//! IPC interface for Vesper

use crate::AudioContext;
use crate::device::AudioDevice;
use crate::stream::StreamInfo;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, error, debug};

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    // Device operations
    ListDevices,
    GetDevice { name: String },
    SetDefaultSink { name: String },
    SetDefaultSource { name: String },

    // Stream operations
    ListStreams,
    GetStream { id: u32 },
    SetStreamVolume { id: u32, volume: u32 },
    SetStreamMute { id: u32, muted: bool },
    MoveStream { id: u32, target: String },

    // Sink/Source operations
    SetVolume { target: String, volume: String },
    SetMute { target: String, muted: bool },
    ToggleMute { target: String },

    // Master operations
    SetMasterVolume { volume: u32 },
    SetMasterMute { muted: bool },

    // Status
    GetStatus,

    // Bluetooth
    ScanBluetooth,
    ConnectBluetooth { address: String },
    DisconnectBluetooth { address: String },
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Devices(Vec<DeviceInfo>),
    Device(DeviceInfo),
    Streams(Vec<StreamInfo>),
    Stream(StreamInfo),
    Status(StatusInfo),
    Muted(bool),
    Error { message: String },
}

/// Device info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub description: String,
    pub device_type: String,
    pub state: String,
    pub is_default: bool,
}

/// Status info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub default_sink: String,
    pub default_source: String,
    pub stream_count: usize,
    pub master_volume: u32,
    pub muted: bool,
}

/// IPC server
pub struct VesperServer {
    socket_path: PathBuf,
    context: AudioContext,
}

impl VesperServer {
    pub fn new(socket_path: PathBuf, context: AudioContext) -> Self {
        Self { socket_path, context }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        info!("Vesper IPC listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let dm = self.context.device_manager.clone();
                    let mixer = self.context.mixer.clone();
                    let clients = self.context.clients.clone();
                    let sinks = self.context.sinks.clone();
                    let sources = self.context.sources.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, dm, mixer, clients, sinks, sources).await {
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
    device_manager: Arc<tokio::sync::RwLock<crate::device::DeviceManager>>,
    mixer: Arc<tokio::sync::RwLock<crate::mixer::Mixer>>,
    clients: Arc<tokio::sync::RwLock<crate::client::ClientManager>>,
    sinks: Arc<tokio::sync::RwLock<std::collections::HashMap<String, crate::sink::Sink>>>,
    sources: Arc<tokio::sync::RwLock<std::collections::HashMap<String, crate::source::Source>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(
                request, &device_manager, &mixer, &clients, &sinks, &sources
            ).await,
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
    device_manager: &tokio::sync::RwLock<crate::device::DeviceManager>,
    mixer: &tokio::sync::RwLock<crate::mixer::Mixer>,
    clients: &tokio::sync::RwLock<crate::client::ClientManager>,
    sinks: &tokio::sync::RwLock<std::collections::HashMap<String, crate::sink::Sink>>,
    sources: &tokio::sync::RwLock<std::collections::HashMap<String, crate::source::Source>>,
) -> IpcResponse {
    match request {
        IpcRequest::ListDevices => {
            let dm = device_manager.read().await;
            let default_sink = dm.default_sink().map(|s| s.to_string());
            let default_source = dm.default_source().map(|s| s.to_string());

            let devices: Vec<DeviceInfo> = dm.all()
                .map(|d| DeviceInfo {
                    name: d.name.clone(),
                    description: d.description.clone(),
                    device_type: d.device_type.to_string(),
                    state: d.state.to_string(),
                    is_default: Some(d.name.as_str()) == default_sink.as_deref() ||
                                Some(d.name.as_str()) == default_source.as_deref(),
                })
                .collect();

            IpcResponse::Devices(devices)
        }

        IpcRequest::ListStreams => {
            let cm = clients.read().await;
            IpcResponse::Streams(cm.stream_info_list())
        }

        IpcRequest::SetDefaultSink { name } => {
            let mut dm = device_manager.write().await;
            match dm.set_default_sink(&name) {
                Ok(()) => IpcResponse::Success { message: format!("Default sink set to {}", name) },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::SetDefaultSource { name } => {
            let mut dm = device_manager.write().await;
            match dm.set_default_source(&name) {
                Ok(()) => IpcResponse::Success { message: format!("Default source set to {}", name) },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::SetVolume { target, volume } => {
            // Parse volume (absolute or relative)
            let new_volume = if volume.starts_with('+') || volume.starts_with('-') {
                let delta: i32 = volume.parse().unwrap_or(0);
                // Get current volume and adjust
                let sink_map = sinks.read().await;
                let current = sink_map.get(&target).map(|s| s.volume as i32).unwrap_or(100);
                (current + delta).max(0).min(150) as u32
            } else {
                volume.trim_end_matches('%').parse().unwrap_or(100)
            };

            // Try sink first
            {
                let mut sink_map = sinks.write().await;
                if let Some(sink) = sink_map.get_mut(&target) {
                    sink.set_volume(new_volume);
                    return IpcResponse::Success {
                        message: format!("Volume set to {}%", new_volume),
                    };
                }
            }

            // Try source
            {
                let mut source_map = sources.write().await;
                if let Some(source) = source_map.get_mut(&target) {
                    source.set_volume(new_volume);
                    return IpcResponse::Success {
                        message: format!("Volume set to {}%", new_volume),
                    };
                }
            }

            IpcResponse::Error { message: format!("Target not found: {}", target) }
        }

        IpcRequest::SetMute { target, muted } => {
            {
                let mut sink_map = sinks.write().await;
                if let Some(sink) = sink_map.get_mut(&target) {
                    sink.set_mute(muted);
                    return IpcResponse::Muted(muted);
                }
            }

            {
                let mut source_map = sources.write().await;
                if let Some(source) = source_map.get_mut(&target) {
                    source.set_mute(muted);
                    return IpcResponse::Muted(muted);
                }
            }

            IpcResponse::Error { message: format!("Target not found: {}", target) }
        }

        IpcRequest::ToggleMute { target } => {
            {
                let mut sink_map = sinks.write().await;
                if let Some(sink) = sink_map.get_mut(&target) {
                    let muted = sink.toggle_mute();
                    return IpcResponse::Muted(muted);
                }
            }

            {
                let mut source_map = sources.write().await;
                if let Some(source) = source_map.get_mut(&target) {
                    let muted = source.toggle_mute();
                    return IpcResponse::Muted(muted);
                }
            }

            IpcResponse::Error { message: format!("Target not found: {}", target) }
        }

        IpcRequest::SetMasterVolume { volume } => {
            let mut m = mixer.write().await;
            m.set_master_volume(volume);
            IpcResponse::Success { message: format!("Master volume set to {}%", volume) }
        }

        IpcRequest::SetMasterMute { muted } => {
            let mut m = mixer.write().await;
            m.set_muted(muted);
            IpcResponse::Muted(muted)
        }

        IpcRequest::GetStatus => {
            let dm = device_manager.read().await;
            let m = mixer.read().await;
            let cm = clients.read().await;

            IpcResponse::Status(StatusInfo {
                default_sink: dm.default_sink().unwrap_or("").to_string(),
                default_source: dm.default_source().unwrap_or("").to_string(),
                stream_count: cm.stream_count(),
                master_volume: m.master_volume(),
                muted: m.is_muted(),
            })
        }

        _ => IpcResponse::Error { message: "Not implemented".to_string() },
    }
}

/// IPC client
pub struct VesperClient {
    socket_path: PathBuf,
}

impl VesperClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let json = serde_json::to_string(&request)?;
        stream.write_all(json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }

    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        match self.send(IpcRequest::ListDevices).await? {
            IpcResponse::Devices(devices) => Ok(devices),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn list_streams(&self) -> Result<Vec<StreamInfo>> {
        match self.send(IpcRequest::ListStreams).await? {
            IpcResponse::Streams(streams) => Ok(streams),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn set_volume(&self, target: &str, volume: &str) -> Result<()> {
        match self.send(IpcRequest::SetVolume {
            target: target.to_string(),
            volume: volume.to_string(),
        }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn set_mute(&self, target: &str, state: &str) -> Result<bool> {
        let muted = match state {
            "on" | "true" | "1" => true,
            "off" | "false" | "0" => false,
            "toggle" => {
                match self.send(IpcRequest::ToggleMute { target: target.to_string() }).await? {
                    IpcResponse::Muted(m) => return Ok(m),
                    IpcResponse::Error { message } => return Err(anyhow::anyhow!(message)),
                    _ => return Err(anyhow::anyhow!("Unexpected response")),
                }
            }
            _ => return Err(anyhow::anyhow!("Invalid mute state")),
        };

        match self.send(IpcRequest::SetMute { target: target.to_string(), muted }).await? {
            IpcResponse::Muted(m) => Ok(m),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn set_default_sink(&self, name: &str) -> Result<()> {
        match self.send(IpcRequest::SetDefaultSink { name: name.to_string() }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn set_default_source(&self, name: &str) -> Result<()> {
        match self.send(IpcRequest::SetDefaultSource { name: name.to_string() }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_status(&self) -> Result<StatusInfo> {
        match self.send(IpcRequest::GetStatus).await? {
            IpcResponse::Status(status) => Ok(status),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}
