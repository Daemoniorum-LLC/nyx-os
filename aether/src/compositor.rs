//! Compositor core
//!
//! Main compositor state and event loop.

use crate::config::AetherConfig;
use crate::input::InputState;
use crate::output::OutputManager;
use crate::render::Renderer;
use crate::security::SecurityManager;
use crate::shell::ShellManager;
use crate::window::WindowManager;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Compositor state
pub struct Compositor {
    /// Configuration
    config: AetherConfig,
    /// Security manager (Guardian integration)
    security: SecurityManager,
    /// Output manager
    outputs: OutputManager,
    /// Window manager
    windows: WindowManager,
    /// Shell manager (xdg-shell, layer-shell, etc.)
    shell: ShellManager,
    /// Input state
    input: InputState,
    /// Renderer
    renderer: Renderer,
    /// Running state
    running: bool,
    /// Start time
    start_time: Instant,
    /// Frame counter
    frame_count: u64,
    /// Windowed mode (for development)
    windowed: bool,
    /// XWayland enabled
    xwayland_enabled: bool,
}

impl Compositor {
    /// Create a new compositor
    pub fn new(
        config: AetherConfig,
        windowed: bool,
        socket_name: Option<String>,
        xwayland: bool,
        guardian_socket: PathBuf,
    ) -> Result<Self> {
        info!("Initializing Aether compositor");

        // Initialize security manager
        let security = SecurityManager::new(&config.security, guardian_socket)?;

        // Initialize output manager
        let outputs = OutputManager::new(&config.display)?;

        // Initialize window manager
        let windows = WindowManager::new(&config.windows)?;

        // Initialize shell manager
        let shell = ShellManager::new()?;

        // Initialize input state
        let input = InputState::new(&config.input)?;

        // Initialize renderer
        let renderer = Renderer::new(&config.render, windowed)?;

        info!("Compositor initialized successfully");

        Ok(Self {
            config,
            security,
            outputs,
            windows,
            shell,
            input,
            renderer,
            running: false,
            start_time: Instant::now(),
            frame_count: 0,
            windowed,
            xwayland_enabled: xwayland && config.xwayland.enabled,
        })
    }

    /// Run the compositor main loop
    pub fn run(&mut self) -> Result<()> {
        self.running = true;
        info!("Starting compositor main loop");

        // In windowed mode, we'd create a window using winit
        // In DRM mode, we'd take over the display directly

        if self.windowed {
            self.run_windowed()?;
        } else {
            self.run_drm()?;
        }

        info!("Compositor shutdown complete");
        Ok(())
    }

    /// Run in windowed mode (for development)
    fn run_windowed(&mut self) -> Result<()> {
        info!("Running in windowed development mode");

        // This would use smithay's winit backend
        // For now, just a placeholder event loop

        while self.running {
            // Process events
            self.process_events()?;

            // Render frame
            self.render_frame()?;

            // Maintain frame rate
            std::thread::sleep(std::time::Duration::from_millis(16)); // ~60fps

            self.frame_count += 1;
        }

        Ok(())
    }

    /// Run with DRM/KMS backend
    fn run_drm(&mut self) -> Result<()> {
        info!("Running with DRM/KMS backend");

        // This would use smithay's drm/udev backends
        // For now, just a placeholder

        while self.running {
            // Process events
            self.process_events()?;

            // Render frame
            self.render_frame()?;

            // Wait for vblank
            // (actual implementation would use DRM page flip)

            self.frame_count += 1;
        }

        Ok(())
    }

    /// Process pending events
    fn process_events(&mut self) -> Result<()> {
        // Process Wayland client events
        // Process input events
        // Process DRM events (mode changes, hotplug)
        // Process XWayland events (if enabled)

        Ok(())
    }

    /// Render a frame
    fn render_frame(&mut self) -> Result<()> {
        // Start frame
        self.renderer.begin_frame()?;

        // Clear background
        self.renderer.clear([0.1, 0.1, 0.15, 1.0])?;

        // Render all visible windows (bottom to top)
        for window in self.windows.visible_windows() {
            // Check if window has render capability
            // (Guardian would mediate this for sensitive operations)

            self.renderer.render_window(&window)?;
        }

        // Render cursors
        self.renderer.render_cursor(&self.input)?;

        // End frame
        self.renderer.end_frame()?;

        Ok(())
    }

    /// Stop the compositor
    pub fn stop(&mut self) {
        info!("Stopping compositor");
        self.running = false;
    }

    /// Get compositor uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get average FPS
    pub fn fps(&self) -> f64 {
        let elapsed = self.uptime().as_secs_f64();
        if elapsed > 0.0 {
            self.frame_count as f64 / elapsed
        } else {
            0.0
        }
    }
}

/// Compositor events
#[derive(Debug, Clone)]
pub enum CompositorEvent {
    /// New client connected
    ClientConnected { client_id: u32 },
    /// Client disconnected
    ClientDisconnected { client_id: u32 },
    /// Window created
    WindowCreated { window_id: u64 },
    /// Window destroyed
    WindowDestroyed { window_id: u64 },
    /// Window focused
    WindowFocused { window_id: u64 },
    /// Output connected
    OutputConnected { output_id: u32, name: String },
    /// Output disconnected
    OutputDisconnected { output_id: u32 },
    /// Mode changed
    ModeChanged { output_id: u32 },
    /// VRR state changed
    VrrChanged { output_id: u32, enabled: bool },
}

/// Client information
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Client ID
    pub id: u32,
    /// Process ID
    pub pid: Option<u32>,
    /// User ID
    pub uid: Option<u32>,
    /// Process name
    pub name: Option<String>,
    /// Connected time
    pub connected_at: Instant,
    /// Windows owned by this client
    pub windows: Vec<u64>,
    /// Granted capabilities
    pub capabilities: Vec<String>,
}
