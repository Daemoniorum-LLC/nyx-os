//! IPC interface for Spectre

use crate::greeter::Greeter;
use crate::seat::SeatManager;
use crate::session::SessionManager;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    ListSessions,
    ListSeats,
    GetSession { id: String },
    LockSession { id: Option<String> },
    UnlockSession { id: String },
    TerminateSession { id: String },
    SwitchSession { id: String },
    ActivateSession { id: String },
    CreateSession {
        username: String,
        session_type: String,
        seat: String,
    },
    SetSessionController { id: String, pid: u32 },
}

/// IPC response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Sessions(Vec<SessionInfo>),
    Seats(Vec<SeatInfo>),
    Session(SessionInfo),
    Error { message: String },
}

/// Session info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub username: String,
    pub uid: u32,
    pub seat: String,
    pub state: String,
    pub session_type: String,
    pub vt: Option<u32>,
    pub tty: Option<String>,
    pub display: Option<String>,
    pub remote_host: Option<String>,
    pub leader_pid: Option<u32>,
    pub created_at: String,
}

/// Seat info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatInfo {
    pub id: String,
    pub active_session: Option<String>,
    pub can_tty: bool,
    pub can_graphical: bool,
}

/// IPC server
pub struct SpectreServer {
    socket_path: PathBuf,
    sessions: Arc<RwLock<SessionManager>>,
    seats: Arc<RwLock<SeatManager>>,
    greeter: Arc<Greeter>,
}

impl SpectreServer {
    pub fn new(
        socket_path: PathBuf,
        sessions: Arc<RwLock<SessionManager>>,
        seats: Arc<RwLock<SeatManager>>,
        greeter: Arc<Greeter>,
    ) -> Self {
        Self {
            socket_path,
            sessions,
            seats,
            greeter,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        info!("Spectre IPC listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let sessions = self.sessions.clone();
                    let seats = self.seats.clone();
                    let greeter = self.greeter.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, sessions, seats, greeter).await {
                            error!("Client handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    sessions: Arc<RwLock<SessionManager>>,
    seats: Arc<RwLock<SeatManager>>,
    greeter: Arc<Greeter>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        debug!("Received: {}", line.trim());

        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &sessions, &seats, &greeter).await,
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
    sessions: &RwLock<SessionManager>,
    seats: &RwLock<SeatManager>,
    _greeter: &Greeter,
) -> IpcResponse {
    match request {
        IpcRequest::ListSessions => {
            let session_mgr = sessions.read().await;
            let infos: Vec<SessionInfo> = session_mgr.all()
                .map(|s| SessionInfo {
                    id: s.id.clone(),
                    username: s.username.clone(),
                    uid: s.uid,
                    seat: s.seat.clone(),
                    state: s.state.as_str().to_string(),
                    session_type: s.session_type.clone(),
                    vt: s.vt,
                    tty: s.tty.clone(),
                    display: s.display.clone(),
                    remote_host: s.remote_host.clone(),
                    leader_pid: s.leader_pid,
                    created_at: s.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                })
                .collect();

            IpcResponse::Sessions(infos)
        }

        IpcRequest::ListSeats => {
            let seat_mgr = seats.read().await;
            let infos: Vec<SeatInfo> = seat_mgr.all()
                .map(|s| SeatInfo {
                    id: s.id.clone(),
                    active_session: s.active_session.clone(),
                    can_tty: s.can_tty,
                    can_graphical: s.can_graphical,
                })
                .collect();

            IpcResponse::Seats(infos)
        }

        IpcRequest::GetSession { id } => {
            let session_mgr = sessions.read().await;
            if let Some(s) = session_mgr.get(&id) {
                IpcResponse::Session(SessionInfo {
                    id: s.id.clone(),
                    username: s.username.clone(),
                    uid: s.uid,
                    seat: s.seat.clone(),
                    state: s.state.as_str().to_string(),
                    session_type: s.session_type.clone(),
                    vt: s.vt,
                    tty: s.tty.clone(),
                    display: s.display.clone(),
                    remote_host: s.remote_host.clone(),
                    leader_pid: s.leader_pid,
                    created_at: s.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                })
            } else {
                IpcResponse::Error {
                    message: format!("Session not found: {}", id),
                }
            }
        }

        IpcRequest::LockSession { id } => {
            let mut session_mgr = sessions.write().await;
            let target_id = if let Some(id) = id {
                id
            } else {
                // Lock active session
                match session_mgr.active_sessions().next() {
                    Some(s) => s.id.clone(),
                    None => {
                        return IpcResponse::Error {
                            message: "No active session".to_string(),
                        };
                    }
                }
            };

            match session_mgr.lock(&target_id) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Locked session {}", target_id),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::UnlockSession { id } => {
            let mut session_mgr = sessions.write().await;
            match session_mgr.unlock(&id) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Unlocked session {}", id),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::TerminateSession { id } => {
            let mut session_mgr = sessions.write().await;
            match session_mgr.end(&id) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Terminated session {}", id),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::SwitchSession { id } => {
            let session_mgr = sessions.read().await;
            let seat_id = session_mgr.get(&id)
                .map(|s| s.seat.clone());
            drop(session_mgr);

            if let Some(seat_id) = seat_id {
                let mut seat_mgr = seats.write().await;
                match seat_mgr.switch_session(&seat_id, &id) {
                    Ok(()) => IpcResponse::Success {
                        message: format!("Switched to session {}", id),
                    },
                    Err(e) => IpcResponse::Error { message: e.to_string() },
                }
            } else {
                IpcResponse::Error {
                    message: format!("Session not found: {}", id),
                }
            }
        }

        IpcRequest::ActivateSession { id } => {
            let mut session_mgr = sessions.write().await;
            match session_mgr.activate(&id) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Activated session {}", id),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::CreateSession { username, session_type, seat } => {
            let user_info = match crate::user::get_user_info(&username) {
                Ok(info) => info,
                Err(e) => {
                    return IpcResponse::Error {
                        message: format!("User not found: {}", e),
                    };
                }
            };

            let mut session_mgr = sessions.write().await;
            match session_mgr.create_session(
                &user_info,
                &seat,
                &session_type,
                crate::session::SessionClass::User,
            ) {
                Ok(session) => IpcResponse::Success {
                    message: format!("Created session {}", session.id),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::SetSessionController { id, pid } => {
            // Would set the session controller PID
            IpcResponse::Success {
                message: format!("Set controller for {} to PID {}", id, pid),
            }
        }
    }
}

/// IPC client
pub struct SpectreClient {
    socket_path: PathBuf,
}

impl SpectreClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
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

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        match self.send(IpcRequest::ListSessions).await? {
            IpcResponse::Sessions(sessions) => Ok(sessions),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn list_seats(&self) -> Result<Vec<SeatInfo>> {
        match self.send(IpcRequest::ListSeats).await? {
            IpcResponse::Seats(seats) => Ok(seats),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn lock_current(&self) -> Result<()> {
        match self.send(IpcRequest::LockSession { id: None }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn logout_current(&self) -> Result<()> {
        // Get current session and terminate
        let sessions = self.list_sessions().await?;
        if let Some(active) = sessions.iter().find(|s| s.state == "active") {
            self.send(IpcRequest::TerminateSession { id: active.id.clone() }).await?;
        }
        Ok(())
    }

    pub async fn switch_session(&self, id: &str) -> Result<()> {
        match self.send(IpcRequest::SwitchSession { id: id.to_string() }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}
