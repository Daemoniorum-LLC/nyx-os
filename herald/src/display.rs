//! Platform-aware notification display
//!
//! Supports:
//! - Freedesktop D-Bus notifications (native Linux, WSL with WSLg)
//! - Windows Toast notifications (WSL with interop)
//! - Console fallback (headless/SSH)

use crate::notification::{Notification, Urgency};
use anyhow::Result;
use libnyx_platform::{Platform, compat::NotificationBackend, wsl};
use std::process::Command;

/// Notification display handler
pub struct NotificationDisplay {
    backend: NotificationBackend,
    platform: Platform,
}

impl NotificationDisplay {
    pub fn new() -> Self {
        let platform = Platform::detect();
        let backend = libnyx_platform::compat::notification_backend();

        tracing::info!(
            "Notification display on {} using {:?}",
            platform.name(),
            backend
        );

        Self { backend, platform }
    }

    /// Display a notification
    pub async fn show(&self, notification: &Notification) -> Result<()> {
        match self.backend {
            NotificationBackend::Freedesktop => {
                self.show_freedesktop(notification).await
            }
            NotificationBackend::WindowsToast => {
                self.show_windows_toast(notification)
            }
            NotificationBackend::Console => {
                self.show_console(notification)
            }
        }
    }

    /// Show notification using Freedesktop D-Bus
    async fn show_freedesktop(&self, notification: &Notification) -> Result<()> {
        // Use notify-send as a simple implementation
        // In production, this would use zbus directly
        let mut cmd = Command::new("notify-send");

        // Set urgency
        let urgency = match notification.urgency {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        };
        cmd.args(["--urgency", urgency]);

        // Set icon
        if let Some(ref icon) = notification.app_icon {
            cmd.args(["--icon", icon]);
        }

        // Set app name
        cmd.args(["--app-name", &notification.app_name]);

        // Set timeout
        if notification.timeout > 0 {
            cmd.args(["--expire-time", &notification.timeout.to_string()]);
        }

        // Add actions as hints (notify-send doesn't support real actions)
        if !notification.actions.is_empty() {
            let actions: Vec<_> = notification.actions.iter()
                .map(|a| a.label.as_str())
                .collect();
            cmd.args(["--hint", &format!("string:actions:{}", actions.join(","))]);
        }

        // Summary and body
        cmd.arg(&notification.summary);
        if let Some(ref body) = notification.body {
            cmd.arg(body);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            tracing::warn!(
                "notify-send failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Show Windows Toast notification via PowerShell
    fn show_windows_toast(&self, notification: &Notification) -> Result<()> {
        let title = notification.summary.replace('"', "'").replace('\n', " ");
        let body = notification.body
            .as_ref()
            .map(|b| b.replace('"', "'").replace('\n', " "))
            .unwrap_or_default();

        // Build PowerShell script for toast notification
        let ps_script = format!(
            r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null

$template = @"
<toast>
    <visual>
        <binding template="ToastText02">
            <text id="1">{}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    <audio src="ms-winsoundevent:Notification.Default"/>
</toast>
"@

$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml($template)
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
$notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Nyx Herald")
$notifier.Show($toast)
"#,
            title, body
        );

        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_script])
            .output()?;

        if !output.status.success() {
            // Fallback to BurntToast if available
            self.try_burnt_toast(notification)?;
        }

        Ok(())
    }

    /// Try using BurntToast PowerShell module
    fn try_burnt_toast(&self, notification: &Notification) -> Result<()> {
        let title = notification.summary.replace('"', "'");
        let body = notification.body
            .as_ref()
            .map(|b| b.replace('"', "'"))
            .unwrap_or_default();

        let ps_script = format!(
            r#"
if (Get-Module -ListAvailable -Name BurntToast) {{
    Import-Module BurntToast
    New-BurntToastNotification -Text "{}", "{}" -AppLogo $null
}} else {{
    # Fallback to basic notification
    [System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms') | Out-Null
    $balloon = New-Object System.Windows.Forms.NotifyIcon
    $balloon.Icon = [System.Drawing.SystemIcons]::Information
    $balloon.BalloonTipTitle = "{}"
    $balloon.BalloonTipText = "{}"
    $balloon.Visible = $true
    $balloon.ShowBalloonTip(5000)
}}
"#,
            title, body, title, body
        );

        Command::new("powershell.exe")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_script])
            .output()?;

        Ok(())
    }

    /// Console fallback for headless environments
    fn show_console(&self, notification: &Notification) -> Result<()> {
        let urgency_icon = match notification.urgency {
            Urgency::Low => "ℹ️",
            Urgency::Normal => "🔔",
            Urgency::Critical => "🚨",
        };

        println!(
            "{} [{}] {}",
            urgency_icon,
            notification.app_name,
            notification.summary
        );

        if let Some(ref body) = notification.body {
            println!("   {}", body);
        }

        Ok(())
    }

    /// Close/dismiss a notification
    pub async fn close(&self, id: u32) -> Result<()> {
        match self.backend {
            NotificationBackend::Freedesktop => {
                // Would call org.freedesktop.Notifications.CloseNotification
                tracing::debug!("Close notification {} via D-Bus", id);
            }
            NotificationBackend::WindowsToast => {
                // Windows toasts auto-dismiss; no manual close needed
                tracing::debug!("Windows toast {} will auto-dismiss", id);
            }
            NotificationBackend::Console => {
                // Console notifications can't be closed
            }
        }
        Ok(())
    }

    /// Get the active backend
    pub fn backend(&self) -> NotificationBackend {
        self.backend
    }

    /// Check if notification actions are supported
    pub fn supports_actions(&self) -> bool {
        matches!(self.backend, NotificationBackend::Freedesktop)
    }

    /// Check if notification sounds are supported
    pub fn supports_sound(&self) -> bool {
        matches!(
            self.backend,
            NotificationBackend::Freedesktop | NotificationBackend::WindowsToast
        )
    }
}

impl Default for NotificationDisplay {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if we can display GUI notifications
pub fn can_display_gui() -> bool {
    let platform = Platform::detect();
    let caps = libnyx_platform::PlatformCapabilities::detect();

    // Check for display server
    if caps.wayland {
        return true;
    }

    // Check for X11
    if std::env::var("DISPLAY").is_ok() {
        return true;
    }

    // Check for WSLg
    if platform.is_wsl() && std::path::Path::new("/mnt/wslg").exists() {
        return true;
    }

    // WSL with interop can use Windows notifications
    if platform.is_wsl() && wsl::interop_enabled() {
        return true;
    }

    false
}
