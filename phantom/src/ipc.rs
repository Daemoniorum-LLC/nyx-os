//! IPC interface for Phantom

use crate::device::{Device, DeviceDatabase, DeviceFilter};
use crate::rule::RuleSet;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

/// IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    ListDevices { subsystem: Option<String> },
    GetDevice { path: String },
    Trigger { subsystem: Option<String>, action: String },
    Monitor,
    TestRules { path: String },
    Settle,
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { message: String },
    Devices(Vec<DeviceInfo>),
    Device(DeviceInfo),
    RuleTest(Vec<(String, String)>),
    Error { message: String },
}

/// Device info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub syspath: String,
    pub devpath: String,
    pub subsystem: Option<String>,
    pub devtype: Option<String>,
    pub devnode: Option<String>,
    pub driver: Option<String>,
    pub sysname: String,
    pub properties: HashMap<String, String>,
}

impl From<&Device> for DeviceInfo {
    fn from(device: &Device) -> Self {
        Self {
            syspath: device.syspath.clone(),
            devpath: device.devpath.clone(),
            subsystem: device.subsystem.clone(),
            devtype: device.devtype.clone(),
            devnode: device.devnode.clone(),
            driver: device.driver.clone(),
            sysname: device.sysname.clone(),
            properties: device.properties.clone(),
        }
    }
}

/// IPC server
pub struct PhantomServer {
    socket_path: PathBuf,
    devices: Arc<RwLock<DeviceDatabase>>,
    rules: Arc<RwLock<RuleSet>>,
}

impl PhantomServer {
    pub fn new(
        socket_path: PathBuf,
        devices: Arc<RwLock<DeviceDatabase>>,
        rules: Arc<RwLock<RuleSet>>,
    ) -> Self {
        Self {
            socket_path,
            devices,
            rules,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        info!("Phantom IPC listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let devices = self.devices.clone();
                    let rules = self.rules.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, devices, rules).await {
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
    devices: Arc<RwLock<DeviceDatabase>>,
    rules: Arc<RwLock<RuleSet>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &devices, &rules).await,
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
    devices: &RwLock<DeviceDatabase>,
    rules: &RwLock<RuleSet>,
) -> IpcResponse {
    match request {
        IpcRequest::ListDevices { subsystem } => {
            let db = devices.read().await;

            let device_list: Vec<DeviceInfo> = if let Some(subsystem) = subsystem {
                db.by_subsystem(&subsystem)
                    .into_iter()
                    .map(DeviceInfo::from)
                    .collect()
            } else {
                db.all().map(DeviceInfo::from).collect()
            };

            IpcResponse::Devices(device_list)
        }

        IpcRequest::GetDevice { path } => {
            let db = devices.read().await;

            if let Some(device) = db.get(&path) {
                IpcResponse::Device(DeviceInfo::from(device))
            } else {
                IpcResponse::Error {
                    message: format!("Device not found: {}", path),
                }
            }
        }

        IpcRequest::Trigger { subsystem, action } => {
            if let Some(subsystem) = subsystem {
                match crate::netlink::trigger_subsystem(&subsystem, &action) {
                    Ok(()) => IpcResponse::Success {
                        message: format!("Triggered {} for {}", action, subsystem),
                    },
                    Err(e) => IpcResponse::Error { message: e.to_string() },
                }
            } else {
                IpcResponse::Success {
                    message: "Use --subsystem to specify target".to_string(),
                }
            }
        }

        IpcRequest::Monitor => {
            // Monitor mode would keep connection open
            IpcResponse::Success {
                message: "Monitor mode not available via IPC".to_string(),
            }
        }

        IpcRequest::TestRules { path } => {
            let db = devices.read().await;
            let rule_set = rules.read().await;

            if let Some(device) = db.get(&path) {
                let matched = rule_set.find_matches(device);

                let results: Vec<(String, String)> = matched.iter()
                    .flat_map(|r| {
                        r.actions.iter().map(|a| {
                            (
                                r.name.clone().unwrap_or_else(|| "unnamed".to_string()),
                                format!("{:?}", a),
                            )
                        })
                    })
                    .collect();

                IpcResponse::RuleTest(results)
            } else {
                IpcResponse::Error {
                    message: format!("Device not found: {}", path),
                }
            }
        }

        IpcRequest::Settle => {
            // Wait for all events to be processed
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            IpcResponse::Success {
                message: "Settled".to_string(),
            }
        }
    }
}

/// IPC client
pub struct PhantomClient {
    socket_path: PathBuf,
}

impl PhantomClient {
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

    pub async fn list_devices(&self, subsystem: Option<&str>) -> Result<Vec<DeviceInfo>> {
        match self.send(IpcRequest::ListDevices {
            subsystem: subsystem.map(String::from),
        }).await? {
            IpcResponse::Devices(devices) => Ok(devices),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn get_device(&self, path: &str) -> Result<DeviceInfo> {
        match self.send(IpcRequest::GetDevice { path: path.to_string() }).await? {
            IpcResponse::Device(device) => Ok(device),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn trigger(&self, subsystem: Option<&str>, action: &str) -> Result<()> {
        match self.send(IpcRequest::Trigger {
            subsystem: subsystem.map(String::from),
            action: action.to_string(),
        }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    pub async fn monitor(&self) -> Result<()> {
        // Would stream events
        println!("Monitor mode - press Ctrl+C to exit");
        tokio::signal::ctrl_c().await?;
        Ok(())
    }

    pub async fn test_rules(&self, path: &str) -> Result<Vec<(String, String)>> {
        match self.send(IpcRequest::TestRules { path: path.to_string() }).await? {
            IpcResponse::RuleTest(results) => Ok(results),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}
