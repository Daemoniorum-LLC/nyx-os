//! Icon system for Nyx OS theme
//!
//! Provides icon paths and helpers for consistent iconography across
//! all Nyx OS applications. Uses a combination of custom icons and
//! Material Design icons.

use serde::{Deserialize, Serialize};

/// Icon identifier for the Nyx icon system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NyxIcon {
    // ═══════════════════════════════════════════════════════════════════════════
    // SYSTEM
    // ═══════════════════════════════════════════════════════════════════════════
    /// Nyx OS logo
    NyxLogo,
    /// Activities/overview
    Activities,
    /// Application grid
    AppGrid,
    /// Search
    Search,
    /// Settings
    Settings,
    /// Power
    Power,
    /// Lock
    Lock,
    /// User/profile
    User,
    /// Menu (hamburger)
    Menu,
    /// More (three dots)
    More,

    // ═══════════════════════════════════════════════════════════════════════════
    // NAVIGATION
    // ═══════════════════════════════════════════════════════════════════════════
    /// Back arrow
    Back,
    /// Forward arrow
    Forward,
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Home
    Home,
    /// Close (X)
    Close,
    /// Maximize
    Maximize,
    /// Minimize
    Minimize,
    /// Expand
    Expand,
    /// Collapse
    Collapse,
    /// Fullscreen
    Fullscreen,
    /// Exit fullscreen
    ExitFullscreen,

    // ═══════════════════════════════════════════════════════════════════════════
    // ACTIONS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Add/plus
    Add,
    /// Remove/minus
    Remove,
    /// Edit/pencil
    Edit,
    /// Delete/trash
    Delete,
    /// Copy
    Copy,
    /// Paste
    Paste,
    /// Cut
    Cut,
    /// Undo
    Undo,
    /// Redo
    Redo,
    /// Save
    Save,
    /// Share
    Share,
    /// Download
    Download,
    /// Upload
    Upload,
    /// Refresh
    Refresh,
    /// Sync
    Sync,
    /// Send
    Send,
    /// Pin
    Pin,
    /// Unpin
    Unpin,
    /// Star/favorite
    Star,
    /// StarFilled
    StarFilled,

    // ═══════════════════════════════════════════════════════════════════════════
    // AUDIO
    // ═══════════════════════════════════════════════════════════════════════════
    /// Volume high
    VolumeHigh,
    /// Volume medium
    VolumeMedium,
    /// Volume low
    VolumeLow,
    /// Volume muted
    VolumeMuted,
    /// Microphone
    Microphone,
    /// Microphone muted
    MicrophoneMuted,
    /// Headphones
    Headphones,
    /// Speaker
    Speaker,

    // ═══════════════════════════════════════════════════════════════════════════
    // DISPLAY
    // ═══════════════════════════════════════════════════════════════════════════
    /// Brightness high
    BrightnessHigh,
    /// Brightness medium
    BrightnessMedium,
    /// Brightness low
    BrightnessLow,
    /// Night light
    NightLight,
    /// Display/monitor
    Display,
    /// External display
    ExternalDisplay,

    // ═══════════════════════════════════════════════════════════════════════════
    // CONNECTIVITY
    // ═══════════════════════════════════════════════════════════════════════════
    /// WiFi connected
    WifiConnected,
    /// WiFi weak
    WifiWeak,
    /// WiFi disconnected
    WifiDisconnected,
    /// Ethernet
    Ethernet,
    /// Bluetooth on
    BluetoothOn,
    /// Bluetooth off
    BluetoothOff,
    /// Bluetooth connected
    BluetoothConnected,
    /// Airplane mode
    AirplaneMode,
    /// VPN
    Vpn,
    /// Hotspot
    Hotspot,

    // ═══════════════════════════════════════════════════════════════════════════
    // POWER & BATTERY
    // ═══════════════════════════════════════════════════════════════════════════
    /// Battery full
    BatteryFull,
    /// Battery high
    BatteryHigh,
    /// Battery medium
    BatteryMedium,
    /// Battery low
    BatteryLow,
    /// Battery critical
    BatteryCritical,
    /// Battery charging
    BatteryCharging,
    /// Power plugged
    PowerPlugged,

    // ═══════════════════════════════════════════════════════════════════════════
    // FILES & FOLDERS
    // ═══════════════════════════════════════════════════════════════════════════
    /// File
    File,
    /// Folder
    Folder,
    /// FolderOpen
    FolderOpen,
    /// Image
    Image,
    /// Video
    Video,
    /// Audio file
    AudioFile,
    /// Document
    Document,
    /// Code file
    Code,
    /// Archive
    Archive,
    /// Cloud
    Cloud,
    /// CloudUpload
    CloudUpload,
    /// CloudDownload
    CloudDownload,

    // ═══════════════════════════════════════════════════════════════════════════
    // COMMUNICATION
    // ═══════════════════════════════════════════════════════════════════════════
    /// Chat/message
    Chat,
    /// Email
    Email,
    /// Notification
    Notification,
    /// NotificationOff
    NotificationOff,
    /// Bell
    Bell,
    /// BellOff
    BellOff,

    // ═══════════════════════════════════════════════════════════════════════════
    // AI & ASSISTANT
    // ═══════════════════════════════════════════════════════════════════════════
    /// AI assistant
    Assistant,
    /// Brain/neural
    Brain,
    /// Sparkle/magic
    Sparkle,
    /// Robot
    Robot,
    /// Wand/magic
    Wand,
    /// Lightning/quick
    Lightning,
    /// Command palette
    Command,

    // ═══════════════════════════════════════════════════════════════════════════
    // STATUS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Check/success
    Check,
    /// CheckCircle
    CheckCircle,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Info
    Info,
    /// Help/question
    Help,
    /// Loading/spinner
    Loading,
    /// Clock
    Clock,
    /// Calendar
    Calendar,

    // ═══════════════════════════════════════════════════════════════════════════
    // WINDOW CONTROLS
    // ═══════════════════════════════════════════════════════════════════════════
    /// Window close
    WindowClose,
    /// Window maximize
    WindowMaximize,
    /// Window minimize
    WindowMinimize,
    /// Window restore
    WindowRestore,

    // ═══════════════════════════════════════════════════════════════════════════
    // MISC
    // ═══════════════════════════════════════════════════════════════════════════
    /// Terminal
    Terminal,
    /// Bug/debug
    Bug,
    /// Key
    Key,
    /// Shield/security
    Shield,
    /// Eye/visible
    Eye,
    /// EyeOff/hidden
    EyeOff,
    /// Link
    Link,
    /// Unlink
    Unlink,
    /// QRCode
    QrCode,
    /// Palette/theme
    Palette,
    /// Language
    Language,
    /// Keyboard
    Keyboard,
    /// Mouse
    Mouse,
    /// Gamepad
    Gamepad,
    /// Print
    Print,
    /// Screenshot
    Screenshot,
}

impl NyxIcon {
    /// Get the Unicode character for this icon (using Material Design icons font)
    pub fn to_char(self) -> char {
        match self {
            // System
            NyxIcon::NyxLogo => '\u{E000}',      // Custom glyph
            NyxIcon::Activities => '\u{E8F4}',  // dashboard
            NyxIcon::AppGrid => '\u{E5C3}',     // apps
            NyxIcon::Search => '\u{E8B6}',      // search
            NyxIcon::Settings => '\u{E8B8}',    // settings
            NyxIcon::Power => '\u{E8AC}',       // power_settings_new
            NyxIcon::Lock => '\u{E897}',        // lock
            NyxIcon::User => '\u{E853}',        // person
            NyxIcon::Menu => '\u{E5D2}',        // menu
            NyxIcon::More => '\u{E5D4}',        // more_vert

            // Navigation
            NyxIcon::Back => '\u{E5C4}',        // arrow_back
            NyxIcon::Forward => '\u{E5C8}',     // arrow_forward
            NyxIcon::Up => '\u{E5C7}',          // arrow_upward
            NyxIcon::Down => '\u{E5C5}',        // arrow_downward
            NyxIcon::Home => '\u{E88A}',        // home
            NyxIcon::Close => '\u{E5CD}',       // close
            NyxIcon::Maximize => '\u{E930}',    // crop_square
            NyxIcon::Minimize => '\u{E931}',    // minimize
            NyxIcon::Expand => '\u{E5CE}',      // expand_more
            NyxIcon::Collapse => '\u{E5CF}',    // expand_less
            NyxIcon::Fullscreen => '\u{E5D0}',  // fullscreen
            NyxIcon::ExitFullscreen => '\u{E5D1}', // fullscreen_exit

            // Actions
            NyxIcon::Add => '\u{E145}',         // add
            NyxIcon::Remove => '\u{E15B}',      // remove
            NyxIcon::Edit => '\u{E3C9}',        // edit
            NyxIcon::Delete => '\u{E872}',      // delete
            NyxIcon::Copy => '\u{E14D}',        // content_copy
            NyxIcon::Paste => '\u{E14F}',       // content_paste
            NyxIcon::Cut => '\u{E14E}',         // content_cut
            NyxIcon::Undo => '\u{E166}',        // undo
            NyxIcon::Redo => '\u{E15A}',        // redo
            NyxIcon::Save => '\u{E161}',        // save
            NyxIcon::Share => '\u{E80D}',       // share
            NyxIcon::Download => '\u{E2C4}',    // file_download
            NyxIcon::Upload => '\u{E2C6}',      // file_upload
            NyxIcon::Refresh => '\u{E5D5}',     // refresh
            NyxIcon::Sync => '\u{E627}',        // sync
            NyxIcon::Send => '\u{E163}',        // send
            NyxIcon::Pin => '\u{E894}',         // push_pin
            NyxIcon::Unpin => '\u{E894}',       // push_pin (outline variant)
            NyxIcon::Star => '\u{E83A}',        // star_border
            NyxIcon::StarFilled => '\u{E838}',  // star

            // Audio
            NyxIcon::VolumeHigh => '\u{E050}',  // volume_up
            NyxIcon::VolumeMedium => '\u{E04D}', // volume_down
            NyxIcon::VolumeLow => '\u{E04E}',   // volume_mute
            NyxIcon::VolumeMuted => '\u{E04F}', // volume_off
            NyxIcon::Microphone => '\u{E029}',  // mic
            NyxIcon::MicrophoneMuted => '\u{E02B}', // mic_off
            NyxIcon::Headphones => '\u{E310}',  // headset
            NyxIcon::Speaker => '\u{E32D}',     // speaker

            // Display
            NyxIcon::BrightnessHigh => '\u{E1AC}', // brightness_high
            NyxIcon::BrightnessMedium => '\u{E1AD}', // brightness_medium
            NyxIcon::BrightnessLow => '\u{E1AE}', // brightness_low
            NyxIcon::NightLight => '\u{EF44}',  // nightlight
            NyxIcon::Display => '\u{E30B}',     // desktop_windows
            NyxIcon::ExternalDisplay => '\u{E30C}', // desktop_mac

            // Connectivity
            NyxIcon::WifiConnected => '\u{E1D8}', // signal_wifi_4_bar
            NyxIcon::WifiWeak => '\u{E1D9}',    // signal_wifi_1_bar
            NyxIcon::WifiDisconnected => '\u{E1DA}', // signal_wifi_off
            NyxIcon::Ethernet => '\u{EA77}',    // settings_ethernet
            NyxIcon::BluetoothOn => '\u{E1A7}', // bluetooth
            NyxIcon::BluetoothOff => '\u{E1A8}', // bluetooth_disabled
            NyxIcon::BluetoothConnected => '\u{E1A6}', // bluetooth_connected
            NyxIcon::AirplaneMode => '\u{E195}', // airplanemode_active
            NyxIcon::Vpn => '\u{E62F}',         // vpn_key
            NyxIcon::Hotspot => '\u{E040}',     // wifi_tethering

            // Power & Battery
            NyxIcon::BatteryFull => '\u{E1A4}', // battery_full
            NyxIcon::BatteryHigh => '\u{EBD2}', // battery_6_bar
            NyxIcon::BatteryMedium => '\u{EBCF}', // battery_4_bar
            NyxIcon::BatteryLow => '\u{EBCD}',  // battery_2_bar
            NyxIcon::BatteryCritical => '\u{E19C}', // battery_alert
            NyxIcon::BatteryCharging => '\u{E1A3}', // battery_charging_full
            NyxIcon::PowerPlugged => '\u{E63E}', // power

            // Files & Folders
            NyxIcon::File => '\u{E24D}',        // insert_drive_file
            NyxIcon::Folder => '\u{E2C7}',      // folder
            NyxIcon::FolderOpen => '\u{E2C8}',  // folder_open
            NyxIcon::Image => '\u{E3F4}',       // image
            NyxIcon::Video => '\u{E04B}',       // videocam
            NyxIcon::AudioFile => '\u{E3A1}',   // audiotrack
            NyxIcon::Document => '\u{E873}',    // description
            NyxIcon::Code => '\u{E86F}',        // code
            NyxIcon::Archive => '\u{EB3F}',     // folder_zip
            NyxIcon::Cloud => '\u{E2BD}',       // cloud
            NyxIcon::CloudUpload => '\u{E2C3}', // cloud_upload
            NyxIcon::CloudDownload => '\u{E2C0}', // cloud_download

            // Communication
            NyxIcon::Chat => '\u{E0B7}',        // chat
            NyxIcon::Email => '\u{E0BE}',       // email
            NyxIcon::Notification => '\u{E7F4}', // notifications
            NyxIcon::NotificationOff => '\u{E7F6}', // notifications_off
            NyxIcon::Bell => '\u{EF52}',        // notifications_active
            NyxIcon::BellOff => '\u{E7F6}',     // notifications_off

            // AI & Assistant
            NyxIcon::Assistant => '\u{EA47}',   // assistant
            NyxIcon::Brain => '\u{F099}',       // psychology
            NyxIcon::Sparkle => '\u{E65F}',     // auto_awesome
            NyxIcon::Robot => '\u{EA36}',       // smart_toy
            NyxIcon::Wand => '\u{EA67}',        // auto_fix_high
            NyxIcon::Lightning => '\u{EA0B}',   // bolt
            NyxIcon::Command => '\u{EACD}',     // terminal

            // Status
            NyxIcon::Check => '\u{E5CA}',       // check
            NyxIcon::CheckCircle => '\u{E86C}', // check_circle
            NyxIcon::Warning => '\u{E002}',     // warning
            NyxIcon::Error => '\u{E000}',       // error
            NyxIcon::Info => '\u{E88E}',        // info
            NyxIcon::Help => '\u{E887}',        // help
            NyxIcon::Loading => '\u{E86A}',     // autorenew
            NyxIcon::Clock => '\u{E8B5}',       // schedule
            NyxIcon::Calendar => '\u{E878}',    // calendar_today

            // Window Controls
            NyxIcon::WindowClose => '\u{E5CD}', // close
            NyxIcon::WindowMaximize => '\u{E3C0}', // crop_din
            NyxIcon::WindowMinimize => '\u{E15B}', // remove
            NyxIcon::WindowRestore => '\u{E3C2}', // crop_free

            // Misc
            NyxIcon::Terminal => '\u{EACD}',    // terminal
            NyxIcon::Bug => '\u{E868}',         // bug_report
            NyxIcon::Key => '\u{E73C}',         // key
            NyxIcon::Shield => '\u{E9E0}',      // shield
            NyxIcon::Eye => '\u{E8F4}',         // visibility
            NyxIcon::EyeOff => '\u{E8F5}',      // visibility_off
            NyxIcon::Link => '\u{E157}',        // link
            NyxIcon::Unlink => '\u{EAD7}',      // link_off
            NyxIcon::QrCode => '\u{EF6B}',      // qr_code
            NyxIcon::Palette => '\u{E40A}',     // palette
            NyxIcon::Language => '\u{E894}',    // language
            NyxIcon::Keyboard => '\u{E312}',    // keyboard
            NyxIcon::Mouse => '\u{E323}',       // mouse
            NyxIcon::Gamepad => '\u{E338}',     // gamepad
            NyxIcon::Print => '\u{E8AD}',       // print
            NyxIcon::Screenshot => '\u{E3B0}',  // screenshot
        }
    }

    /// Get the icon name as a string (useful for logging/debugging)
    pub fn name(self) -> &'static str {
        match self {
            NyxIcon::NyxLogo => "nyx_logo",
            NyxIcon::Activities => "activities",
            NyxIcon::AppGrid => "app_grid",
            NyxIcon::Search => "search",
            NyxIcon::Settings => "settings",
            NyxIcon::Power => "power",
            NyxIcon::Lock => "lock",
            NyxIcon::User => "user",
            NyxIcon::Menu => "menu",
            NyxIcon::More => "more",
            NyxIcon::Back => "back",
            NyxIcon::Forward => "forward",
            NyxIcon::Up => "up",
            NyxIcon::Down => "down",
            NyxIcon::Home => "home",
            NyxIcon::Close => "close",
            NyxIcon::Maximize => "maximize",
            NyxIcon::Minimize => "minimize",
            NyxIcon::Expand => "expand",
            NyxIcon::Collapse => "collapse",
            NyxIcon::Fullscreen => "fullscreen",
            NyxIcon::ExitFullscreen => "exit_fullscreen",
            NyxIcon::Add => "add",
            NyxIcon::Remove => "remove",
            NyxIcon::Edit => "edit",
            NyxIcon::Delete => "delete",
            NyxIcon::Copy => "copy",
            NyxIcon::Paste => "paste",
            NyxIcon::Cut => "cut",
            NyxIcon::Undo => "undo",
            NyxIcon::Redo => "redo",
            NyxIcon::Save => "save",
            NyxIcon::Share => "share",
            NyxIcon::Download => "download",
            NyxIcon::Upload => "upload",
            NyxIcon::Refresh => "refresh",
            NyxIcon::Sync => "sync",
            NyxIcon::Send => "send",
            NyxIcon::Pin => "pin",
            NyxIcon::Unpin => "unpin",
            NyxIcon::Star => "star",
            NyxIcon::StarFilled => "star_filled",
            NyxIcon::VolumeHigh => "volume_high",
            NyxIcon::VolumeMedium => "volume_medium",
            NyxIcon::VolumeLow => "volume_low",
            NyxIcon::VolumeMuted => "volume_muted",
            NyxIcon::Microphone => "microphone",
            NyxIcon::MicrophoneMuted => "microphone_muted",
            NyxIcon::Headphones => "headphones",
            NyxIcon::Speaker => "speaker",
            NyxIcon::BrightnessHigh => "brightness_high",
            NyxIcon::BrightnessMedium => "brightness_medium",
            NyxIcon::BrightnessLow => "brightness_low",
            NyxIcon::NightLight => "night_light",
            NyxIcon::Display => "display",
            NyxIcon::ExternalDisplay => "external_display",
            NyxIcon::WifiConnected => "wifi_connected",
            NyxIcon::WifiWeak => "wifi_weak",
            NyxIcon::WifiDisconnected => "wifi_disconnected",
            NyxIcon::Ethernet => "ethernet",
            NyxIcon::BluetoothOn => "bluetooth_on",
            NyxIcon::BluetoothOff => "bluetooth_off",
            NyxIcon::BluetoothConnected => "bluetooth_connected",
            NyxIcon::AirplaneMode => "airplane_mode",
            NyxIcon::Vpn => "vpn",
            NyxIcon::Hotspot => "hotspot",
            NyxIcon::BatteryFull => "battery_full",
            NyxIcon::BatteryHigh => "battery_high",
            NyxIcon::BatteryMedium => "battery_medium",
            NyxIcon::BatteryLow => "battery_low",
            NyxIcon::BatteryCritical => "battery_critical",
            NyxIcon::BatteryCharging => "battery_charging",
            NyxIcon::PowerPlugged => "power_plugged",
            NyxIcon::File => "file",
            NyxIcon::Folder => "folder",
            NyxIcon::FolderOpen => "folder_open",
            NyxIcon::Image => "image",
            NyxIcon::Video => "video",
            NyxIcon::AudioFile => "audio_file",
            NyxIcon::Document => "document",
            NyxIcon::Code => "code",
            NyxIcon::Archive => "archive",
            NyxIcon::Cloud => "cloud",
            NyxIcon::CloudUpload => "cloud_upload",
            NyxIcon::CloudDownload => "cloud_download",
            NyxIcon::Chat => "chat",
            NyxIcon::Email => "email",
            NyxIcon::Notification => "notification",
            NyxIcon::NotificationOff => "notification_off",
            NyxIcon::Bell => "bell",
            NyxIcon::BellOff => "bell_off",
            NyxIcon::Assistant => "assistant",
            NyxIcon::Brain => "brain",
            NyxIcon::Sparkle => "sparkle",
            NyxIcon::Robot => "robot",
            NyxIcon::Wand => "wand",
            NyxIcon::Lightning => "lightning",
            NyxIcon::Command => "command",
            NyxIcon::Check => "check",
            NyxIcon::CheckCircle => "check_circle",
            NyxIcon::Warning => "warning",
            NyxIcon::Error => "error",
            NyxIcon::Info => "info",
            NyxIcon::Help => "help",
            NyxIcon::Loading => "loading",
            NyxIcon::Clock => "clock",
            NyxIcon::Calendar => "calendar",
            NyxIcon::WindowClose => "window_close",
            NyxIcon::WindowMaximize => "window_maximize",
            NyxIcon::WindowMinimize => "window_minimize",
            NyxIcon::WindowRestore => "window_restore",
            NyxIcon::Terminal => "terminal",
            NyxIcon::Bug => "bug",
            NyxIcon::Key => "key",
            NyxIcon::Shield => "shield",
            NyxIcon::Eye => "eye",
            NyxIcon::EyeOff => "eye_off",
            NyxIcon::Link => "link",
            NyxIcon::Unlink => "unlink",
            NyxIcon::QrCode => "qr_code",
            NyxIcon::Palette => "palette",
            NyxIcon::Language => "language",
            NyxIcon::Keyboard => "keyboard",
            NyxIcon::Mouse => "mouse",
            NyxIcon::Gamepad => "gamepad",
            NyxIcon::Print => "print",
            NyxIcon::Screenshot => "screenshot",
        }
    }
}
