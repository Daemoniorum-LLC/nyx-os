//! Platform detection and compatibility layer
//!
//! Provides runtime detection for different environments (native Linux, WSL1, WSL2, containers)
//! and abstracts platform-specific functionality.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Detected platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Native Linux (bare metal or VM)
    NativeLinux,
    /// Windows Subsystem for Linux version 1 (translation layer)
    Wsl1,
    /// Windows Subsystem for Linux version 2 (Hyper-V VM)
    Wsl2,
    /// Docker or other container runtime
    Container,
    /// Unknown/unsupported platform
    Unknown,
}

/// Platform capabilities
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    /// Can use real cgroups v2
    pub cgroups_v2: bool,
    /// Can use nftables/iptables
    pub netfilter: bool,
    /// Can create network namespaces
    pub network_namespaces: bool,
    /// Can use Unix domain sockets
    pub unix_sockets: bool,
    /// Has access to /dev
    pub devfs: bool,
    /// Can use ptrace for debugging
    pub ptrace: bool,
    /// Can use kernel keyring
    pub keyring: bool,
    /// Can run Wayland compositor
    pub wayland: bool,
    /// Has GPU access
    pub gpu: bool,
    /// Can use inotify/fanotify
    pub inotify: bool,
    /// Has systemd available
    pub systemd: bool,
    /// Can access Windows filesystem (WSL)
    pub windows_interop: bool,
    /// Windows drive mount path (e.g., /mnt/c)
    pub windows_drives: Option<String>,
}

static PLATFORM: OnceLock<Platform> = OnceLock::new();
static CAPABILITIES: OnceLock<PlatformCapabilities> = OnceLock::new();

impl Platform {
    /// Detect the current platform
    pub fn detect() -> Self {
        *PLATFORM.get_or_init(|| {
            // Check for WSL first
            if let Some(wsl) = detect_wsl() {
                return wsl;
            }

            // Check for container
            if detect_container() {
                return Platform::Container;
            }

            // Check if we're on Linux at all
            if cfg!(target_os = "linux") {
                Platform::NativeLinux
            } else {
                Platform::Unknown
            }
        })
    }

    /// Get platform name for logging
    pub fn name(&self) -> &'static str {
        match self {
            Platform::NativeLinux => "Linux",
            Platform::Wsl1 => "WSL1",
            Platform::Wsl2 => "WSL2",
            Platform::Container => "Container",
            Platform::Unknown => "Unknown",
        }
    }

    /// Check if running under WSL (any version)
    pub fn is_wsl(&self) -> bool {
        matches!(self, Platform::Wsl1 | Platform::Wsl2)
    }

    /// Check if platform has full kernel access
    pub fn has_full_kernel(&self) -> bool {
        matches!(self, Platform::NativeLinux | Platform::Wsl2)
    }
}

/// Detect WSL environment
fn detect_wsl() -> Option<Platform> {
    // Method 1: Check /proc/version
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let version_lower = version.to_lowercase();

        if version_lower.contains("microsoft") || version_lower.contains("wsl") {
            // Distinguish WSL1 vs WSL2
            // WSL2 has a real kernel, WSL1 uses translation

            // Method: Check for WSL2-specific features
            if Path::new("/run/WSL").exists() {
                return Some(Platform::Wsl2);
            }

            // Check kernel version - WSL2 uses 5.x+
            if let Some(kernel_ver) = parse_kernel_version(&version) {
                if kernel_ver.0 >= 5 {
                    return Some(Platform::Wsl2);
                }
            }

            // Check for Hyper-V (WSL2 indicator)
            if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
                if cpuinfo.contains("hypervisor") {
                    return Some(Platform::Wsl2);
                }
            }

            return Some(Platform::Wsl1);
        }
    }

    // Method 2: Check WSL-specific environment variables
    if std::env::var("WSL_DISTRO_NAME").is_ok() ||
       std::env::var("WSL_INTEROP").is_ok() {
        // Default to WSL2 as it's more common now
        return Some(Platform::Wsl2);
    }

    // Method 3: Check for /mnt/c (Windows drive mount)
    if Path::new("/mnt/c/Windows").exists() {
        return Some(Platform::Wsl2);
    }

    None
}

/// Parse kernel version from /proc/version
fn parse_kernel_version(version: &str) -> Option<(u32, u32, u32)> {
    // Format: "Linux version X.Y.Z-..."
    let parts: Vec<&str> = version.split_whitespace().collect();

    for (i, part) in parts.iter().enumerate() {
        if *part == "version" && i + 1 < parts.len() {
            let ver_str = parts[i + 1];
            let nums: Vec<&str> = ver_str.split(['.', '-']).collect();

            if nums.len() >= 3 {
                let major = nums[0].parse().ok()?;
                let minor = nums[1].parse().ok()?;
                let patch = nums[2].parse().ok()?;
                return Some((major, minor, patch));
            }
        }
    }

    None
}

/// Detect container environment
fn detect_container() -> bool {
    // Check for Docker
    if Path::new("/.dockerenv").exists() {
        return true;
    }

    // Check cgroup for container indicators
    if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
        if cgroup.contains("docker") ||
           cgroup.contains("lxc") ||
           cgroup.contains("kubepods") ||
           cgroup.contains("containerd") {
            return true;
        }
    }

    // Check for container environment variable
    if std::env::var("container").is_ok() {
        return true;
    }

    false
}

impl PlatformCapabilities {
    /// Detect capabilities for current platform
    pub fn detect() -> Self {
        CAPABILITIES.get_or_init(|| {
            let platform = Platform::detect();

            match platform {
                Platform::NativeLinux => Self::native_linux(),
                Platform::Wsl2 => Self::wsl2(),
                Platform::Wsl1 => Self::wsl1(),
                Platform::Container => Self::container(),
                Platform::Unknown => Self::minimal(),
            }
        }).clone()
    }

    fn native_linux() -> Self {
        Self {
            cgroups_v2: check_cgroups_v2(),
            netfilter: check_netfilter(),
            network_namespaces: check_namespaces(),
            unix_sockets: true,
            devfs: Path::new("/dev").exists(),
            ptrace: true,
            keyring: true,
            wayland: check_wayland_possible(),
            gpu: check_gpu(),
            inotify: true,
            systemd: check_systemd(),
            windows_interop: false,
            windows_drives: None,
        }
    }

    fn wsl2() -> Self {
        Self {
            cgroups_v2: check_cgroups_v2(),
            netfilter: true,  // WSL2 has full netfilter
            network_namespaces: true,
            unix_sockets: true,
            devfs: true,
            ptrace: true,
            keyring: false,  // Limited in WSL
            wayland: check_wslg(),
            gpu: check_wsl_gpu(),
            inotify: true,
            systemd: check_systemd(),  // WSL2 can have systemd now
            windows_interop: true,
            windows_drives: Some("/mnt".to_string()),
        }
    }

    fn wsl1() -> Self {
        Self {
            cgroups_v2: false,  // WSL1 doesn't have real cgroups
            netfilter: false,   // No kernel netfilter in WSL1
            network_namespaces: false,
            unix_sockets: true,
            devfs: true,
            ptrace: false,  // Limited in WSL1
            keyring: false,
            wayland: false,  // No WSLg in WSL1
            gpu: false,
            inotify: true,  // Emulated
            systemd: false,
            windows_interop: true,
            windows_drives: Some("/mnt".to_string()),
        }
    }

    fn container() -> Self {
        Self {
            cgroups_v2: false,  // Usually limited
            netfilter: false,
            network_namespaces: false,
            unix_sockets: true,
            devfs: true,
            ptrace: false,
            keyring: false,
            wayland: false,
            gpu: check_gpu(),  // Might have GPU passthrough
            inotify: true,
            systemd: false,
            windows_interop: false,
            windows_drives: None,
        }
    }

    fn minimal() -> Self {
        Self {
            cgroups_v2: false,
            netfilter: false,
            network_namespaces: false,
            unix_sockets: true,
            devfs: false,
            ptrace: false,
            keyring: false,
            wayland: false,
            gpu: false,
            inotify: false,
            systemd: false,
            windows_interop: false,
            windows_drives: None,
        }
    }
}

fn check_cgroups_v2() -> bool {
    Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
}

fn check_netfilter() -> bool {
    Path::new("/proc/net/nf_conntrack").exists() ||
    Path::new("/proc/net/ip_tables_names").exists() ||
    std::process::Command::new("nft")
        .arg("list")
        .arg("tables")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_namespaces() -> bool {
    Path::new("/proc/self/ns/net").exists()
}

fn check_wayland_possible() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok() ||
    std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false)
}

fn check_wslg() -> bool {
    // WSLg provides Wayland through /mnt/wslg
    Path::new("/mnt/wslg").exists() ||
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn check_gpu() -> bool {
    // Check for any GPU in /dev/dri
    Path::new("/dev/dri/card0").exists() ||
    Path::new("/dev/dri/renderD128").exists()
}

fn check_wsl_gpu() -> bool {
    // WSL2 GPU support via /dev/dxg
    Path::new("/dev/dxg").exists() || check_gpu()
}

fn check_systemd() -> bool {
    // Check if systemd is PID 1
    if let Ok(cmdline) = std::fs::read_to_string("/proc/1/cmdline") {
        if cmdline.contains("systemd") {
            return true;
        }
    }

    // Check for systemd socket
    Path::new("/run/systemd/system").exists()
}

/// WSL-specific utilities
pub mod wsl {
    use super::*;
    use std::process::Command;

    /// Get Windows username
    pub fn windows_user() -> Option<String> {
        if !Platform::detect().is_wsl() {
            return None;
        }

        // Try WSL_USER first
        if let Ok(user) = std::env::var("WSL_USER") {
            return Some(user);
        }

        // Fall back to wslvar
        Command::new("wslvar")
            .arg("USERNAME")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    /// Get Windows home directory path
    pub fn windows_home() -> Option<String> {
        if !Platform::detect().is_wsl() {
            return None;
        }

        // Try USERPROFILE
        Command::new("wslvar")
            .arg("USERPROFILE")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
            .and_then(|path| wslpath(&path))
    }

    /// Convert Windows path to WSL path
    pub fn wslpath(windows_path: &str) -> Option<String> {
        Command::new("wslpath")
            .arg("-u")
            .arg(windows_path)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    /// Convert WSL path to Windows path
    pub fn to_windows_path(linux_path: &str) -> Option<String> {
        Command::new("wslpath")
            .arg("-w")
            .arg(linux_path)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    /// Open file/URL with Windows default application
    pub fn open_with_windows(path: &str) -> std::io::Result<()> {
        // Use Windows explorer.exe or cmd /c start
        let win_path = to_windows_path(path).unwrap_or_else(|| path.to_string());

        Command::new("cmd.exe")
            .args(["/c", "start", "", &win_path])
            .spawn()?;

        Ok(())
    }

    /// Run a Windows executable from WSL
    pub fn run_windows_exe(exe: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
        Command::new(exe)
            .args(args)
            .output()
    }

    /// Get the WSL distribution name
    pub fn distro_name() -> Option<String> {
        std::env::var("WSL_DISTRO_NAME").ok()
    }

    /// Check if WSL interop is enabled
    pub fn interop_enabled() -> bool {
        std::env::var("WSL_INTEROP").is_ok() ||
        Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
    }

    /// Send Windows toast notification
    pub fn send_toast(title: &str, message: &str) -> std::io::Result<()> {
        let ps_script = format!(
            r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$template = @"
<toast>
    <visual>
        <binding template="ToastText02">
            <text id="1">{}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
</toast>
"@
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml($template)
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Nyx").Show($toast)"#,
            title.replace('"', "'"),
            message.replace('"', "'")
        );

        Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &ps_script])
            .output()?;

        Ok(())
    }
}

/// Platform-aware service implementation helpers
pub mod compat {
    use super::*;

    /// Get appropriate firewall backend
    pub fn firewall_backend() -> FirewallBackend {
        let caps = PlatformCapabilities::detect();

        if caps.netfilter {
            FirewallBackend::Nftables
        } else if Platform::detect().is_wsl() {
            FirewallBackend::WindowsFirewall
        } else {
            FirewallBackend::None
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub enum FirewallBackend {
        Nftables,
        Iptables,
        WindowsFirewall,
        None,
    }

    /// Get appropriate notification backend
    pub fn notification_backend() -> NotificationBackend {
        let platform = Platform::detect();
        let caps = PlatformCapabilities::detect();

        if platform.is_wsl() && wsl::interop_enabled() {
            NotificationBackend::WindowsToast
        } else if caps.wayland || std::env::var("DISPLAY").is_ok() {
            NotificationBackend::Freedesktop
        } else {
            NotificationBackend::Console
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub enum NotificationBackend {
        Freedesktop,
        WindowsToast,
        Console,
    }

    /// Get process isolation method
    pub fn isolation_method() -> IsolationMethod {
        let caps = PlatformCapabilities::detect();

        if caps.cgroups_v2 && caps.network_namespaces {
            IsolationMethod::Full
        } else if caps.cgroups_v2 {
            IsolationMethod::CgroupsOnly
        } else {
            IsolationMethod::None
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub enum IsolationMethod {
        Full,        // cgroups + namespaces
        CgroupsOnly, // Just resource limits
        None,        // No isolation available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::detect();
        println!("Detected platform: {:?}", platform);
        assert_ne!(platform, Platform::Unknown);
    }

    #[test]
    fn test_capabilities() {
        let caps = PlatformCapabilities::detect();
        println!("Platform capabilities: {:?}", caps);
        assert!(caps.unix_sockets); // Should always be true on Linux
    }
}
