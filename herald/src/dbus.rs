//! D-Bus interface for freedesktop notifications

use crate::notification::{CloseReason, HintValue, Notification, NotificationAction, Urgency};
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// D-Bus notification events
#[derive(Debug, Clone)]
pub enum DbusEvent {
    Notify(NotifyRequest),
    CloseNotification(u32),
    GetCapabilities,
    GetServerInformation,
}

/// Notify request from D-Bus
#[derive(Debug, Clone)]
pub struct NotifyRequest {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub hints: HashMap<String, HintValue>,
    pub expire_timeout: i32,
}

/// D-Bus notification signals
#[derive(Debug, Clone)]
pub enum DbusSignal {
    NotificationClosed { id: u32, reason: u32 },
    ActionInvoked { id: u32, action_key: String },
}

/// D-Bus notification server interface
/// Implements org.freedesktop.Notifications
pub struct NotificationDbusServer {
    event_tx: mpsc::Sender<DbusEvent>,
    signal_rx: Option<mpsc::Receiver<DbusSignal>>,
}

impl NotificationDbusServer {
    pub fn new() -> (Self, mpsc::Receiver<DbusEvent>, mpsc::Sender<DbusSignal>) {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (signal_tx, signal_rx) = mpsc::channel(100);

        let server = Self {
            event_tx,
            signal_rx: Some(signal_rx),
        };

        (server, event_rx, signal_tx)
    }

    /// Get server capabilities
    pub fn capabilities() -> Vec<&'static str> {
        vec![
            "actions",           // Support notification actions
            "body",              // Support body text
            "body-hyperlinks",   // Support hyperlinks in body
            "body-markup",       // Support basic markup in body
            "icon-static",       // Support static icons
            "persistence",       // Support persistent notifications
            "sound",             // Support sounds
        ]
    }

    /// Get server information
    pub fn server_info() -> (&'static str, &'static str, &'static str, &'static str) {
        (
            "Herald",            // Name
            "Nyx",               // Vendor
            "0.1.0",             // Version
            "1.2",               // Spec version
        )
    }

    /// Parse hints from D-Bus variant map
    pub fn parse_hints(hints: &HashMap<String, HintValue>) -> (Urgency, HashMap<String, HintValue>) {
        let urgency = hints.get("urgency")
            .and_then(|v| match v {
                HintValue::Byte(b) => Some(*b),
                HintValue::Uint(u) => Some(*u as u8),
                HintValue::Int(i) => Some(*i as u8),
                _ => None,
            })
            .map(|u| match u {
                0 => Urgency::Low,
                1 => Urgency::Normal,
                2 => Urgency::Critical,
                _ => Urgency::Normal,
            })
            .unwrap_or(Urgency::Normal);

        (urgency, hints.clone())
    }

    /// Convert NotifyRequest to Notification
    pub fn to_notification(request: NotifyRequest, id: u32) -> Notification {
        let (urgency, hints) = Self::parse_hints(&request.hints);

        // Parse actions (pairs of id, label)
        let actions: Vec<NotificationAction> = request.actions
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(NotificationAction {
                        id: chunk[0].clone(),
                        label: chunk[1].clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let mut notification = Notification::new(id, &request.app_name, &request.summary);
        notification.body = if request.body.is_empty() { None } else { Some(request.body) };
        notification.app_icon = if request.app_icon.is_empty() { None } else { Some(request.app_icon) };
        notification.urgency = urgency;
        notification.timeout = request.expire_timeout;
        notification.actions = actions;
        notification.hints = hints;
        notification.replaces_id = if request.replaces_id > 0 { Some(request.replaces_id) } else { None };

        // Extract common hints
        if let Some(HintValue::String(cat)) = notification.hints.get("category") {
            notification.category = Some(cat.clone());
        }
        if let Some(HintValue::Bool(true)) = notification.hints.get("resident") {
            notification.resident = true;
        }
        if let Some(HintValue::Bool(true)) = notification.hints.get("transient") {
            notification.transient = true;
        }

        notification
    }

    /// Send notification closed signal
    pub async fn emit_closed(signal_tx: &mpsc::Sender<DbusSignal>, id: u32, reason: CloseReason) {
        let _ = signal_tx.send(DbusSignal::NotificationClosed {
            id,
            reason: reason.to_code(),
        }).await;
    }

    /// Send action invoked signal
    pub async fn emit_action(signal_tx: &mpsc::Sender<DbusSignal>, id: u32, action_key: &str) {
        let _ = signal_tx.send(DbusSignal::ActionInvoked {
            id,
            action_key: action_key.to_string(),
        }).await;
    }
}

impl Default for NotificationDbusServer {
    fn default() -> Self {
        let (server, _, _) = Self::new();
        server
    }
}

/// Simplified D-Bus mock for non-systemd environments
/// In production, this would use zbus or dbus-rs
pub mod mock {
    use super::*;
    use tokio::net::UnixListener;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use std::path::Path;

    /// Mock D-Bus server using Unix sockets
    pub struct MockDbusServer {
        event_tx: mpsc::Sender<DbusEvent>,
    }

    impl MockDbusServer {
        pub fn new(event_tx: mpsc::Sender<DbusEvent>) -> Self {
            Self { event_tx }
        }

        pub async fn start(&self, socket_path: &Path) -> Result<()> {
            let _ = std::fs::remove_file(socket_path);
            let listener = UnixListener::bind(socket_path)?;

            tracing::info!("Mock D-Bus server listening on {:?}", socket_path);

            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    let event_tx = self.event_tx.clone();

                    tokio::spawn(async move {
                        let (reader, mut writer) = stream.into_split();
                        let mut reader = BufReader::new(reader);
                        let mut line = String::new();

                        while reader.read_line(&mut line).await.is_ok() && !line.is_empty() {
                            // Parse simple JSON-RPC style messages
                            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                                let method = msg.get("method").and_then(|v| v.as_str());

                                match method {
                                    Some("Notify") => {
                                        if let Some(params) = msg.get("params") {
                                            let request = NotifyRequest {
                                                app_name: params.get("app_name")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string(),
                                                replaces_id: params.get("replaces_id")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0) as u32,
                                                app_icon: params.get("app_icon")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string(),
                                                summary: params.get("summary")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string(),
                                                body: params.get("body")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string(),
                                                actions: Vec::new(),
                                                hints: HashMap::new(),
                                                expire_timeout: params.get("timeout")
                                                    .and_then(|v| v.as_i64())
                                                    .unwrap_or(-1) as i32,
                                            };

                                            let _ = event_tx.send(DbusEvent::Notify(request)).await;
                                        }
                                    }
                                    Some("CloseNotification") => {
                                        if let Some(id) = msg.get("params")
                                            .and_then(|p| p.get("id"))
                                            .and_then(|v| v.as_u64())
                                        {
                                            let _ = event_tx.send(DbusEvent::CloseNotification(id as u32)).await;
                                        }
                                    }
                                    Some("GetCapabilities") => {
                                        let _ = event_tx.send(DbusEvent::GetCapabilities).await;
                                        let response = serde_json::json!({
                                            "capabilities": NotificationDbusServer::capabilities()
                                        });
                                        let _ = writer.write_all(response.to_string().as_bytes()).await;
                                        let _ = writer.write_all(b"\n").await;
                                    }
                                    Some("GetServerInformation") => {
                                        let _ = event_tx.send(DbusEvent::GetServerInformation).await;
                                        let info = NotificationDbusServer::server_info();
                                        let response = serde_json::json!({
                                            "name": info.0,
                                            "vendor": info.1,
                                            "version": info.2,
                                            "spec_version": info.3
                                        });
                                        let _ = writer.write_all(response.to_string().as_bytes()).await;
                                        let _ = writer.write_all(b"\n").await;
                                    }
                                    _ => {}
                                }
                            }

                            line.clear();
                        }
                    });
                }
            }
        }
    }
}
