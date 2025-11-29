//! Application launching and action execution

use crate::desktop::DesktopEntry;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use tokio::sync::mpsc;

/// Application launcher
pub struct Launcher {
    running: HashMap<u32, RunningApp>,
    event_tx: mpsc::Sender<LaunchEvent>,
}

pub struct RunningApp {
    pub app_id: String,
    pub pid: u32,
    pub started: std::time::Instant,
}

#[derive(Debug, Clone)]
pub enum LaunchEvent {
    Started { app_id: String, pid: u32 },
    Exited { app_id: String, pid: u32, code: i32 },
    Failed { app_id: String, error: String },
}

impl Launcher {
    pub fn new() -> (Self, mpsc::Receiver<LaunchEvent>) {
        let (event_tx, event_rx) = mpsc::channel(100);

        let launcher = Self {
            running: HashMap::new(),
            event_tx,
        };

        (launcher, event_rx)
    }

    /// Launch an application
    pub async fn launch(&mut self, entry: &DesktopEntry, files: &[String]) -> Result<u32> {
        // Check TryExec if specified
        if let Some(ref try_exec) = entry.try_exec {
            if !which::which(try_exec).is_ok() {
                return Err(anyhow!("TryExec failed: {} not found", try_exec));
            }
        }

        // Get the command
        let command = entry.get_command(files);
        let parts: Vec<&str> = command.split_whitespace().collect();

        if parts.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        let program = parts[0];
        let args = &parts[1..];

        // Build command
        let mut cmd = Command::new(program);
        cmd.args(args);

        // Set working directory if specified
        if let Some(ref path) = entry.path {
            cmd.current_dir(path);
        }

        // Handle terminal applications
        if entry.terminal {
            cmd = self.wrap_in_terminal(&command)?;
        }

        // Detach from our process group
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Set startup notification
        if entry.startup_notify {
            // Would set DESKTOP_STARTUP_ID here
        }

        // Spawn process
        let child = cmd.spawn().map_err(|e| anyhow!("Failed to spawn: {}", e))?;
        let pid = child.id();

        // Track running app
        self.running.insert(pid, RunningApp {
            app_id: entry.id.clone(),
            pid,
            started: std::time::Instant::now(),
        });

        // Send launch event
        let _ = self.event_tx.send(LaunchEvent::Started {
            app_id: entry.id.clone(),
            pid,
        }).await;

        // Spawn watcher for exit
        let event_tx = self.event_tx.clone();
        let app_id = entry.id.clone();

        tokio::spawn(async move {
            // Wait for process to exit
            // Note: In a real implementation, we'd properly wait on the child
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // For now, just assume successful launch
            // A proper implementation would monitor the process
        });

        tracing::info!("Launched {} (PID: {})", entry.name, pid);
        Ok(pid)
    }

    /// Launch an application action
    pub async fn launch_action(
        &mut self,
        entry: &DesktopEntry,
        action_id: &str,
        files: &[String],
    ) -> Result<u32> {
        let command = entry.get_action_command(action_id, files)
            .ok_or_else(|| anyhow!("Action not found: {}", action_id))?;

        let parts: Vec<&str> = command.split_whitespace().collect();

        if parts.is_empty() {
            return Err(anyhow!("Empty action command"));
        }

        let mut cmd = Command::new(parts[0]);
        cmd.args(&parts[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn()?;
        let pid = child.id();

        let _ = self.event_tx.send(LaunchEvent::Started {
            app_id: format!("{}:{}", entry.id, action_id),
            pid,
        }).await;

        Ok(pid)
    }

    /// Wrap command in terminal emulator
    fn wrap_in_terminal(&self, command: &str) -> Result<Command> {
        // Try common terminal emulators
        let terminals = [
            ("alacritty", vec!["-e"]),
            ("kitty", vec!["--"]),
            ("gnome-terminal", vec!["--"]),
            ("konsole", vec!["-e"]),
            ("xfce4-terminal", vec!["-e"]),
            ("xterm", vec!["-e"]),
        ];

        for (term, args) in terminals {
            if which::which(term).is_ok() {
                let mut cmd = Command::new(term);
                cmd.args(&args);
                cmd.arg("sh").arg("-c").arg(command);
                return Ok(cmd);
            }
        }

        Err(anyhow!("No terminal emulator found"))
    }

    /// Get running applications
    pub fn running_apps(&self) -> Vec<&RunningApp> {
        self.running.values().collect()
    }

    /// Check if an app is running
    pub fn is_running(&self, app_id: &str) -> bool {
        self.running.values().any(|app| app.app_id == app_id)
    }

    /// Get PID of running app
    pub fn get_pid(&self, app_id: &str) -> Option<u32> {
        self.running.values()
            .find(|app| app.app_id == app_id)
            .map(|app| app.pid)
    }
}

impl Default for Launcher {
    fn default() -> Self {
        let (launcher, _) = Self::new();
        launcher
    }
}

/// Quick launcher for simple commands
pub async fn quick_launch(command: &str) -> Result<u32> {
    let parts: Vec<&str> = command.split_whitespace().collect();

    if parts.is_empty() {
        return Err(anyhow!("Empty command"));
    }

    let mut cmd = Command::new(parts[0]);
    cmd.args(&parts[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn()?;
    Ok(child.id())
}

/// Open file with default application
pub async fn open_with_default(path: &str) -> Result<u32> {
    // Try xdg-open on Linux
    let mut cmd = Command::new("xdg-open");
    cmd.arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().map_err(|e| anyhow!("Failed to open: {}", e))?;
    Ok(child.id())
}

/// Open URL in default browser
pub async fn open_url(url: &str) -> Result<u32> {
    open_with_default(url).await
}

/// Calculator action
pub fn calculate(expr: &str) -> Option<f64> {
    // Simple expression evaluator
    let expr = expr.trim();

    // Basic operations
    if let Some((left, right)) = expr.split_once('+') {
        let l: f64 = left.trim().parse().ok()?;
        let r: f64 = right.trim().parse().ok()?;
        return Some(l + r);
    }

    if let Some((left, right)) = expr.split_once('-') {
        let l: f64 = left.trim().parse().ok()?;
        let r: f64 = right.trim().parse().ok()?;
        return Some(l - r);
    }

    if let Some((left, right)) = expr.split_once('*') {
        let l: f64 = left.trim().parse().ok()?;
        let r: f64 = right.trim().parse().ok()?;
        return Some(l * r);
    }

    if let Some((left, right)) = expr.split_once('/') {
        let l: f64 = left.trim().parse().ok()?;
        let r: f64 = right.trim().parse().ok()?;
        if r != 0.0 {
            return Some(l / r);
        }
    }

    None
}

/// Command actions for special queries
pub enum CommandAction {
    Launch(String),
    Calculate(String),
    OpenUrl(String),
    OpenFile(String),
    Search(String, String),  // engine, query
    Define(String),
}

pub fn parse_command(input: &str) -> Option<CommandAction> {
    let input = input.trim();

    // Calculator pattern
    if input.chars().all(|c| c.is_ascii_digit() || "+-*/. ()".contains(c)) {
        if input.contains(|c: char| "+-*/".contains(c)) {
            return Some(CommandAction::Calculate(input.to_string()));
        }
    }

    // URL pattern
    if input.starts_with("http://") || input.starts_with("https://") {
        return Some(CommandAction::OpenUrl(input.to_string()));
    }

    // File path pattern
    if input.starts_with('/') || input.starts_with("~/") {
        return Some(CommandAction::OpenFile(input.to_string()));
    }

    // Search patterns
    if let Some(query) = input.strip_prefix("g ") {
        return Some(CommandAction::Search("google".to_string(), query.to_string()));
    }

    if let Some(query) = input.strip_prefix("ddg ") {
        return Some(CommandAction::Search("duckduckgo".to_string(), query.to_string()));
    }

    if let Some(word) = input.strip_prefix("define ") {
        return Some(CommandAction::Define(word.to_string()));
    }

    None
}
