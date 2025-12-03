//! Message types for Nyx Shell

use crate::workspace::WorkspaceId;

/// Main shell messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Tick for updating time and system status
    Tick,

    /// Panel messages
    Panel(PanelMessage),

    /// Dock messages
    Dock(DockMessage),

    /// Workspace messages
    Workspace(WorkspaceMessage),

    /// System events
    System(SystemMessage),

    /// Toggle control center visibility
    ToggleControlCenter,

    /// Toggle assistant visibility
    ToggleAssistant,

    /// Show activities/overview
    ShowActivities,

    /// Hide activities/overview
    HideActivities,

    /// Font loaded
    FontLoaded(Result<(), iced::font::Error>),
}

/// Panel-specific messages
#[derive(Debug, Clone)]
pub enum PanelMessage {
    /// Activities button clicked
    ActivitiesClicked,
    /// Workspace clicked
    WorkspaceClicked(WorkspaceId),
    /// Tray icon clicked
    TrayIconClicked(String),
    /// Clock clicked (show calendar)
    ClockClicked,
    /// User menu clicked
    UserMenuClicked,
    /// Quick settings toggle
    QuickSettingsClicked,
}

/// Dock-specific messages
#[derive(Debug, Clone)]
pub enum DockMessage {
    /// App icon clicked
    AppClicked(String),
    /// App icon right-clicked (context menu)
    AppRightClicked(String),
    /// App icon hovered
    AppHovered(Option<String>),
    /// Pin/unpin app
    TogglePin(String),
    /// Launch app
    LaunchApp(String),
    /// Focus app window
    FocusApp(String),
    /// Close app
    CloseApp(String),
}

/// Workspace-specific messages
#[derive(Debug, Clone)]
pub enum WorkspaceMessage {
    /// Switch to workspace
    Switch(WorkspaceId),
    /// Create new workspace
    Create,
    /// Remove workspace
    Remove(WorkspaceId),
    /// Rename workspace
    Rename(WorkspaceId, String),
    /// Move window to workspace
    MoveWindow(WorkspaceId),
    /// Reorder workspaces
    Reorder(WorkspaceId, usize),
}

/// System events
#[derive(Debug, Clone)]
pub enum SystemMessage {
    /// Battery status update
    BatteryUpdate(BatteryStatus),
    /// Network status update
    NetworkUpdate(NetworkStatus),
    /// Audio status update
    AudioUpdate(AudioStatus),
    /// Bluetooth status update
    BluetoothUpdate(BluetoothStatus),
    /// Power profile update
    PowerProfileUpdate(PowerProfile),
    /// New notification
    NotificationReceived(Notification),
    /// Notification dismissed
    NotificationDismissed(String),
}

/// Battery status
#[derive(Debug, Clone, Default)]
pub struct BatteryStatus {
    /// Battery percentage (0-100)
    pub percentage: u8,
    /// Is charging
    pub charging: bool,
    /// Is plugged in
    pub plugged: bool,
    /// Time remaining (minutes)
    pub time_remaining: Option<u32>,
}

/// Network status
#[derive(Debug, Clone, Default)]
pub struct NetworkStatus {
    /// Is connected
    pub connected: bool,
    /// Connection type
    pub connection_type: ConnectionType,
    /// SSID for WiFi
    pub ssid: Option<String>,
    /// Signal strength (0-100)
    pub signal_strength: u8,
    /// VPN active
    pub vpn_active: bool,
}

/// Connection type
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectionType {
    #[default]
    Disconnected,
    Ethernet,
    Wifi,
    Cellular,
}

/// Audio status
#[derive(Debug, Clone, Default)]
pub struct AudioStatus {
    /// Volume (0-100)
    pub volume: u8,
    /// Is muted
    pub muted: bool,
    /// Active output device
    pub output_device: Option<String>,
    /// Microphone active
    pub mic_active: bool,
    /// Microphone muted
    pub mic_muted: bool,
}

/// Bluetooth status
#[derive(Debug, Clone, Default)]
pub struct BluetoothStatus {
    /// Is enabled
    pub enabled: bool,
    /// Is discovering
    pub discovering: bool,
    /// Connected devices
    pub connected_devices: Vec<String>,
}

/// Power profile
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PowerProfile {
    /// Power saver mode
    PowerSaver,
    /// Balanced mode
    #[default]
    Balanced,
    /// Performance mode
    Performance,
}

/// Notification
#[derive(Debug, Clone)]
pub struct Notification {
    /// Unique ID
    pub id: String,
    /// Application name
    pub app_name: String,
    /// Summary/title
    pub summary: String,
    /// Body text
    pub body: Option<String>,
    /// Icon name or path
    pub icon: Option<String>,
    /// Urgency level
    pub urgency: NotificationUrgency,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Local>,
    /// Actions available
    pub actions: Vec<(String, String)>,
}

/// Notification urgency
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}
