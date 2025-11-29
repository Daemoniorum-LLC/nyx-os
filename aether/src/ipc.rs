//! IPC interface
//!
//! Control interface for Aether compositor.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aether IPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AetherRequest {
    /// Get compositor status
    Status,
    /// List outputs
    ListOutputs,
    /// Configure output
    ConfigureOutput {
        name: String,
        enabled: Option<bool>,
        position: Option<(i32, i32)>,
        resolution: Option<(u32, u32)>,
        refresh_rate: Option<u32>,
        scale: Option<f32>,
    },
    /// List windows
    ListWindows,
    /// Focus window
    FocusWindow { id: u64 },
    /// Close window
    CloseWindow { id: u64 },
    /// Move window
    MoveWindow { id: u64, x: i32, y: i32 },
    /// Resize window
    ResizeWindow { id: u64, width: u32, height: u32 },
    /// Set window state
    SetWindowState { id: u64, state: String },
    /// Take screenshot
    Screenshot { output: Option<String> },
    /// Set DPMS state
    SetDpms { output: Option<String>, state: String },
    /// Reload configuration
    ReloadConfig,
    /// Shutdown compositor
    Shutdown,
}

/// Aether IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AetherResponse {
    /// Status response
    Status {
        version: String,
        uptime_secs: u64,
        frame_count: u64,
        fps: f64,
        clients: u32,
        windows: u32,
        outputs: u32,
    },
    /// Output list
    Outputs {
        outputs: Vec<OutputInfo>,
    },
    /// Window list
    Windows {
        windows: Vec<WindowInfo>,
    },
    /// Screenshot data
    Screenshot {
        width: u32,
        height: u32,
        format: String,
        data: String, // Base64 encoded
    },
    /// Success
    Ok {
        message: String,
    },
    /// Error
    Error {
        message: String,
    },
}

/// Output info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
    pub make: String,
    pub model: String,
    pub enabled: bool,
    pub position: (i32, i32),
    pub resolution: (u32, u32),
    pub refresh_rate: u32,
    pub scale: f32,
    pub dpms_state: String,
}

/// Window info for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub state: String,
    pub focused: bool,
    pub client_pid: Option<u32>,
}

/// Event types sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AetherEvent {
    /// Output connected
    OutputConnected { output: OutputInfo },
    /// Output disconnected
    OutputDisconnected { id: u32 },
    /// Output changed
    OutputChanged { output: OutputInfo },
    /// Window created
    WindowCreated { window: WindowInfo },
    /// Window destroyed
    WindowDestroyed { id: u64 },
    /// Window changed
    WindowChanged { window: WindowInfo },
    /// Window focused
    WindowFocused { id: u64 },
    /// Compositor shutdown
    Shutdown,
}
