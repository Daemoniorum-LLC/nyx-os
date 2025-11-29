//! Aether configuration

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Aether configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AetherConfig {
    /// Display configuration
    #[serde(default)]
    pub display: DisplayConfig,

    /// Input configuration
    #[serde(default)]
    pub input: InputConfig,

    /// Rendering configuration
    #[serde(default)]
    pub render: RenderConfig,

    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,

    /// Window management configuration
    #[serde(default)]
    pub windows: WindowConfig,

    /// XWayland configuration
    #[serde(default)]
    pub xwayland: XWaylandConfig,
}

impl Default for AetherConfig {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            input: InputConfig::default(),
            render: RenderConfig::default(),
            security: SecurityConfig::default(),
            windows: WindowConfig::default(),
            xwayland: XWaylandConfig::default(),
        }
    }
}

/// Display configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Preferred refresh rate (Hz)
    #[serde(default = "default_refresh")]
    pub refresh_rate: u32,

    /// Enable VRR (Variable Refresh Rate)
    #[serde(default)]
    pub vrr_enabled: bool,

    /// HDR mode
    #[serde(default)]
    pub hdr_mode: HdrMode,

    /// Scale factor for HiDPI
    #[serde(default = "default_scale")]
    pub scale_factor: f32,

    /// Output configurations
    #[serde(default)]
    pub outputs: Vec<OutputConfig>,

    /// Power saving settings
    #[serde(default)]
    pub dpms: DpmsConfig,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            refresh_rate: default_refresh(),
            vrr_enabled: false,
            hdr_mode: HdrMode::Off,
            scale_factor: default_scale(),
            outputs: Vec::new(),
            dpms: DpmsConfig::default(),
        }
    }
}

fn default_refresh() -> u32 {
    60
}

fn default_scale() -> f32 {
    1.0
}

/// HDR mode
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HdrMode {
    #[default]
    Off,
    Sdr,
    Hdr10,
    HdrLinear,
}

/// Per-output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Output name (e.g., "HDMI-A-1")
    pub name: String,
    /// Enable this output
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Position (x, y)
    #[serde(default)]
    pub position: (i32, i32),
    /// Resolution (width, height)
    pub resolution: Option<(u32, u32)>,
    /// Refresh rate override
    pub refresh_rate: Option<u32>,
    /// Scale factor override
    pub scale_factor: Option<f32>,
    /// Rotation
    #[serde(default)]
    pub transform: Transform,
}

/// Display transform/rotation
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// DPMS (Display Power Management Signaling) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpmsConfig {
    /// Enable DPMS
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Standby timeout (seconds)
    #[serde(default = "default_standby")]
    pub standby_secs: u32,
    /// Suspend timeout (seconds)
    #[serde(default = "default_suspend")]
    pub suspend_secs: u32,
    /// Off timeout (seconds)
    #[serde(default = "default_off")]
    pub off_secs: u32,
}

impl Default for DpmsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            standby_secs: default_standby(),
            suspend_secs: default_suspend(),
            off_secs: default_off(),
        }
    }
}

fn default_standby() -> u32 {
    300 // 5 minutes
}

fn default_suspend() -> u32 {
    600 // 10 minutes
}

fn default_off() -> u32 {
    900 // 15 minutes
}

/// Input configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Keyboard configuration
    #[serde(default)]
    pub keyboard: KeyboardConfig,

    /// Mouse/pointer configuration
    #[serde(default)]
    pub pointer: PointerConfig,

    /// Touchpad configuration
    #[serde(default)]
    pub touchpad: TouchpadConfig,

    /// Touchscreen configuration
    #[serde(default)]
    pub touchscreen: TouchscreenConfig,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            keyboard: KeyboardConfig::default(),
            pointer: PointerConfig::default(),
            touchpad: TouchpadConfig::default(),
            touchscreen: TouchscreenConfig::default(),
        }
    }
}

/// Keyboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardConfig {
    /// XKB layout
    #[serde(default = "default_layout")]
    pub layout: String,
    /// XKB variant
    #[serde(default)]
    pub variant: String,
    /// XKB options
    #[serde(default)]
    pub options: String,
    /// Repeat delay (ms)
    #[serde(default = "default_repeat_delay")]
    pub repeat_delay: u32,
    /// Repeat rate (chars/sec)
    #[serde(default = "default_repeat_rate")]
    pub repeat_rate: u32,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            variant: String::new(),
            options: String::new(),
            repeat_delay: default_repeat_delay(),
            repeat_rate: default_repeat_rate(),
        }
    }
}

fn default_layout() -> String {
    "us".into()
}

fn default_repeat_delay() -> u32 {
    400
}

fn default_repeat_rate() -> u32 {
    25
}

/// Pointer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerConfig {
    /// Acceleration profile
    #[serde(default)]
    pub accel_profile: AccelProfile,
    /// Acceleration speed (-1.0 to 1.0)
    #[serde(default)]
    pub accel_speed: f64,
    /// Natural scrolling
    #[serde(default)]
    pub natural_scroll: bool,
    /// Left-handed mode
    #[serde(default)]
    pub left_handed: bool,
}

impl Default for PointerConfig {
    fn default() -> Self {
        Self {
            accel_profile: AccelProfile::Adaptive,
            accel_speed: 0.0,
            natural_scroll: false,
            left_handed: false,
        }
    }
}

/// Acceleration profile
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccelProfile {
    Flat,
    #[default]
    Adaptive,
}

/// Touchpad configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouchpadConfig {
    /// Enable tap-to-click
    #[serde(default = "default_true")]
    pub tap_to_click: bool,
    /// Enable tap-and-drag
    #[serde(default = "default_true")]
    pub tap_and_drag: bool,
    /// Disable while typing
    #[serde(default = "default_true")]
    pub disable_while_typing: bool,
    /// Natural scrolling
    #[serde(default = "default_true")]
    pub natural_scroll: bool,
    /// Two-finger scroll
    #[serde(default = "default_true")]
    pub two_finger_scroll: bool,
    /// Click method
    #[serde(default)]
    pub click_method: ClickMethod,
}

impl Default for TouchpadConfig {
    fn default() -> Self {
        Self {
            tap_to_click: true,
            tap_and_drag: true,
            disable_while_typing: true,
            natural_scroll: true,
            two_finger_scroll: true,
            click_method: ClickMethod::Clickfinger,
        }
    }
}

/// Click method
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickMethod {
    ButtonAreas,
    #[default]
    Clickfinger,
}

/// Touchscreen configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TouchscreenConfig {
    /// Output to map touchscreen to
    pub output: Option<String>,
    /// Calibration matrix
    pub calibration: Option<[f32; 6]>,
}

/// Rendering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// Renderer backend
    #[serde(default)]
    pub backend: RenderBackend,

    /// Enable VSync
    #[serde(default = "default_true")]
    pub vsync: bool,

    /// Triple buffering
    #[serde(default = "default_true")]
    pub triple_buffer: bool,

    /// GPU device (auto-detect if None)
    pub gpu_device: Option<String>,

    /// Enable direct scanout
    #[serde(default = "default_true")]
    pub direct_scanout: bool,

    /// Max FPS (0 = unlimited)
    #[serde(default)]
    pub max_fps: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            backend: RenderBackend::OpenGl,
            vsync: true,
            triple_buffer: true,
            gpu_device: None,
            direct_scanout: true,
            max_fps: 0,
        }
    }
}

/// Render backend
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderBackend {
    #[default]
    OpenGl,
    Vulkan,
    Software,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable Guardian integration
    #[serde(default = "default_true")]
    pub guardian_enabled: bool,

    /// Require capability for screen capture
    #[serde(default = "default_true")]
    pub capture_requires_cap: bool,

    /// Require capability for input grab
    #[serde(default = "default_true")]
    pub input_grab_requires_cap: bool,

    /// Require capability for window management
    #[serde(default)]
    pub wm_requires_cap: bool,

    /// Allow privileged Wayland protocols
    #[serde(default)]
    pub privileged_protocols: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            guardian_enabled: true,
            capture_requires_cap: true,
            input_grab_requires_cap: true,
            wm_requires_cap: false,
            privileged_protocols: Vec::new(),
        }
    }
}

/// Window management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Focus mode
    #[serde(default)]
    pub focus_mode: FocusMode,

    /// Enable window decorations
    #[serde(default = "default_true")]
    pub decorations: bool,

    /// Window border width
    #[serde(default = "default_border")]
    pub border_width: u32,

    /// Active window border color
    #[serde(default = "default_active_color")]
    pub active_border_color: String,

    /// Inactive window border color
    #[serde(default = "default_inactive_color")]
    pub inactive_border_color: String,

    /// Gap between windows
    #[serde(default)]
    pub gap: u32,

    /// Outer gap (screen edge)
    #[serde(default)]
    pub outer_gap: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            focus_mode: FocusMode::ClickToFocus,
            decorations: true,
            border_width: default_border(),
            active_border_color: default_active_color(),
            inactive_border_color: default_inactive_color(),
            gap: 0,
            outer_gap: 0,
        }
    }
}

/// Focus mode
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusMode {
    #[default]
    ClickToFocus,
    FollowMouse,
    FocusFollowsMouse,
}

fn default_border() -> u32 {
    2
}

fn default_active_color() -> String {
    "#7c3aed".into() // Purple (matching DaemonOS theme)
}

fn default_inactive_color() -> String {
    "#404040".into()
}

/// XWayland configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XWaylandConfig {
    /// Enable XWayland
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Lazy start (start when first X11 client connects)
    #[serde(default = "default_true")]
    pub lazy: bool,

    /// Force software cursor for X11 windows
    #[serde(default)]
    pub force_software_cursor: bool,
}

impl Default for XWaylandConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            lazy: true,
            force_software_cursor: false,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Load configuration from file
pub fn load_config(path: &Path) -> Result<AetherConfig> {
    if path.exists() {
        let contents = std::fs::read_to_string(path)?;
        let config: AetherConfig = serde_yaml::from_str(&contents)?;
        info!("Loaded configuration from {}", path.display());
        Ok(config)
    } else {
        info!("No configuration file found, using defaults");
        Ok(AetherConfig::default())
    }
}
