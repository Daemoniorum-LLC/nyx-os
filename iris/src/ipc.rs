//! IPC interface for Iris

use crate::backlight::BacklightInfo;
use crate::display::DisplayInfo;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcRequest {
    /// List all displays
    ListDisplays,

    /// Get display info
    GetDisplay { name: String },

    /// Set display mode
    SetMode {
        name: String,
        width: u32,
        height: u32,
        refresh: f32,
    },

    /// Enable/disable display
    SetEnabled { name: String, enabled: bool },

    /// Set primary display
    SetPrimary { name: String },

    /// Set display position
    SetPosition { name: String, x: i32, y: i32 },

    /// Set display rotation
    SetRotation { name: String, rotation: u16 },

    /// Get backlight info
    GetBacklight,

    /// Set brightness percentage
    SetBrightness { percent: u8 },

    /// Increase brightness
    IncreaseBrightness { step: u8 },

    /// Decrease brightness
    DecreaseBrightness { step: u8 },

    /// Get night light status
    GetNightLight,

    /// Set night light enabled
    SetNightLight { enabled: bool },

    /// Get daemon status
    GetStatus,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// Night light status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightLightStatus {
    pub enabled: bool,
    pub active: bool,
    pub temperature: u32,
}

/// Daemon status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub version: String,
    pub displays: Vec<DisplayInfo>,
    pub backlight: Option<BacklightInfo>,
    pub night_light: NightLightStatus,
}

/// IPC handler trait
pub trait IpcHandler: Send + Sync {
    fn list_displays(&self) -> Vec<DisplayInfo>;
    fn get_display(&self, name: &str) -> Option<DisplayInfo>;
    fn set_mode(&self, name: &str, width: u32, height: u32, refresh: f32) -> Result<()>;
    fn set_enabled(&self, name: &str, enabled: bool) -> Result<()>;
    fn set_primary(&self, name: &str) -> Result<()>;
    fn set_position(&self, name: &str, x: i32, y: i32) -> Result<()>;
    fn set_rotation(&self, name: &str, rotation: u16) -> Result<()>;
    fn get_backlight(&self) -> Option<BacklightInfo>;
    fn set_brightness(&self, percent: u8) -> impl std::future::Future<Output = Result<()>> + Send;
    fn increase_brightness(&self, step: u8) -> impl std::future::Future<Output = Result<u8>> + Send;
    fn decrease_brightness(&self, step: u8) -> impl std::future::Future<Output = Result<u8>> + Send;
    fn get_night_light(&self) -> NightLightStatus;
    fn set_night_light(&self, enabled: bool);
    fn get_status(&self) -> DaemonStatus;
}

/// IPC server
pub struct IpcServer<H: IpcHandler> {
    socket_path: String,
    handler: Arc<H>,
}

impl<H: IpcHandler + 'static> IpcServer<H> {
    pub fn new(socket_path: impl Into<String>, handler: H) -> Self {
        Self {
            socket_path: socket_path.into(),
            handler: Arc::new(handler),
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = std::path::Path::new(&self.socket_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Iris IPC listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handler = Arc::clone(&self.handler);
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, handler).await {
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

async fn handle_client<H: IpcHandler>(stream: UnixStream, handler: Arc<H>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, handler.as_ref()).await,
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

async fn process_request<H: IpcHandler>(request: IpcRequest, handler: &H) -> IpcResponse {
    match request {
        IpcRequest::ListDisplays => {
            let displays = handler.list_displays();
            IpcResponse::Success {
                data: serde_json::to_value(displays).unwrap(),
            }
        }

        IpcRequest::GetDisplay { name } => match handler.get_display(&name) {
            Some(display) => IpcResponse::Success {
                data: serde_json::to_value(display).unwrap(),
            },
            None => IpcResponse::Error {
                message: format!("Display not found: {}", name),
            },
        },

        IpcRequest::SetMode {
            name,
            width,
            height,
            refresh,
        } => match handler.set_mode(&name, width, height, refresh) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"display": name, "mode": format!("{}x{}@{}", width, height, refresh)}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SetEnabled { name, enabled } => match handler.set_enabled(&name, enabled) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"display": name, "enabled": enabled}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SetPrimary { name } => match handler.set_primary(&name) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"primary": name}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SetPosition { name, x, y } => match handler.set_position(&name, x, y) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"display": name, "position": [x, y]}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::SetRotation { name, rotation } => match handler.set_rotation(&name, rotation) {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"display": name, "rotation": rotation}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::GetBacklight => match handler.get_backlight() {
            Some(info) => IpcResponse::Success {
                data: serde_json::to_value(info).unwrap(),
            },
            None => IpcResponse::Error {
                message: "No backlight device available".to_string(),
            },
        },

        IpcRequest::SetBrightness { percent } => match handler.set_brightness(percent).await {
            Ok(()) => IpcResponse::Success {
                data: serde_json::json!({"brightness": percent}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::IncreaseBrightness { step } => match handler.increase_brightness(step).await {
            Ok(new) => IpcResponse::Success {
                data: serde_json::json!({"brightness": new}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::DecreaseBrightness { step } => match handler.decrease_brightness(step).await {
            Ok(new) => IpcResponse::Success {
                data: serde_json::json!({"brightness": new}),
            },
            Err(e) => IpcResponse::Error {
                message: e.to_string(),
            },
        },

        IpcRequest::GetNightLight => {
            let status = handler.get_night_light();
            IpcResponse::Success {
                data: serde_json::to_value(status).unwrap(),
            }
        }

        IpcRequest::SetNightLight { enabled } => {
            handler.set_night_light(enabled);
            IpcResponse::Success {
                data: serde_json::json!({"night_light": enabled}),
            }
        }

        IpcRequest::GetStatus => {
            let status = handler.get_status();
            IpcResponse::Success {
                data: serde_json::to_value(status).unwrap(),
            }
        }
    }
}

/// IPC client
pub struct IpcClient {
    socket_path: String,
}

impl IpcClient {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
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

    pub async fn list_displays(&self) -> Result<Vec<DisplayInfo>> {
        match self.send(IpcRequest::ListDisplays).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get_backlight(&self) -> Result<BacklightInfo> {
        match self.send(IpcRequest::GetBacklight).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn set_brightness(&self, percent: u8) -> Result<()> {
        match self.send(IpcRequest::SetBrightness { percent }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn get_status(&self) -> Result<DaemonStatus> {
        match self.send(IpcRequest::GetStatus).await? {
            IpcResponse::Success { data } => Ok(serde_json::from_value(data)?),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }
}
