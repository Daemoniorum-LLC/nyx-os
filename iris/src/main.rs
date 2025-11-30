//! Iris - Display management daemon for DaemonOS
//!
//! Provides:
//! - Display detection and configuration
//! - Multi-monitor support
//! - Backlight/brightness control
//! - Night light (color temperature)

mod backlight;
mod config;
mod display;
mod ipc;

use crate::backlight::{BacklightInfo, BacklightManager};
use crate::config::IrisConfig;
use crate::display::{DisplayInfo, DisplayManager};
use crate::ipc::{DaemonStatus, IpcHandler, IpcServer, NightLightStatus};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, RwLock};
use tracing::info;

/// Iris - Display management daemon
#[derive(Parser, Debug)]
#[command(name = "irisd", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/iris.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/iris/iris.sock")]
    socket: PathBuf,

    /// Debug mode
    #[arg(short, long)]
    debug: bool,
}

/// Daemon state
struct IrisState {
    config: IrisConfig,
    display_manager: RwLock<DisplayManager>,
    backlight_manager: BacklightManager,
    night_light_enabled: AtomicBool,
}

impl IrisState {
    fn new(config: IrisConfig) -> Result<Self> {
        let mut display_manager = DisplayManager::new(config.displays.clone());
        display_manager.detect()?;

        Ok(Self {
            backlight_manager: BacklightManager::new(config.backlight.clone()),
            night_light_enabled: AtomicBool::new(config.color.night_light),
            display_manager: RwLock::new(display_manager),
            config,
        })
    }
}

impl IpcHandler for IrisState {
    fn list_displays(&self) -> Vec<DisplayInfo> {
        self.display_manager.read().unwrap().list().into_iter().cloned().collect()
    }

    fn get_display(&self, name: &str) -> Option<DisplayInfo> {
        self.display_manager.read().unwrap().get(name).cloned()
    }

    fn set_mode(&self, name: &str, width: u32, height: u32, refresh: f32) -> Result<()> {
        self.display_manager.write().unwrap().set_mode(name, width, height, refresh)
    }

    fn set_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        self.display_manager.write().unwrap().set_enabled(name, enabled)
    }

    fn set_primary(&self, name: &str) -> Result<()> {
        self.display_manager.write().unwrap().set_primary(name)
    }

    fn set_position(&self, name: &str, x: i32, y: i32) -> Result<()> {
        self.display_manager.write().unwrap().set_position(name, x, y)
    }

    fn set_rotation(&self, name: &str, rotation: u16) -> Result<()> {
        self.display_manager.write().unwrap().set_rotation(name, rotation)
    }

    fn get_backlight(&self) -> Option<BacklightInfo> {
        self.backlight_manager.get_info()
    }

    async fn set_brightness(&self, percent: u8) -> Result<()> {
        self.backlight_manager.set_brightness(percent).await
    }

    async fn increase_brightness(&self, step: u8) -> Result<u8> {
        self.backlight_manager.increase_brightness(step).await
    }

    async fn decrease_brightness(&self, step: u8) -> Result<u8> {
        self.backlight_manager.decrease_brightness(step).await
    }

    fn get_night_light(&self) -> NightLightStatus {
        let enabled = self.night_light_enabled.load(Ordering::Relaxed);
        NightLightStatus {
            enabled,
            active: enabled, // Simplified - would check time in real impl
            temperature: if enabled {
                self.config.color.night_temperature
            } else {
                self.config.color.day_temperature
            },
        }
    }

    fn set_night_light(&self, enabled: bool) {
        self.night_light_enabled.store(enabled, Ordering::Relaxed);
        info!("Night light {}", if enabled { "enabled" } else { "disabled" });
    }

    fn get_status(&self) -> DaemonStatus {
        DaemonStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            displays: self.list_displays(),
            backlight: self.get_backlight(),
            night_light: self.get_night_light(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Iris v{} starting", env!("CARGO_PKG_VERSION"));

    let config = IrisConfig::load(&args.config)?;
    let state = Arc::new(IrisState::new(config)?);

    // Start IPC server
    let socket_path = args.socket.to_string_lossy().to_string();
    let server = IpcServer::new(socket_path, Arc::try_unwrap(state).unwrap_or_else(|arc| (*arc).clone()));

    info!("Iris ready");
    server.run().await
}

impl Clone for IrisState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            display_manager: RwLock::new(DisplayManager::new(self.config.displays.clone())),
            backlight_manager: BacklightManager::new(self.config.backlight.clone()),
            night_light_enabled: AtomicBool::new(self.night_light_enabled.load(Ordering::Relaxed)),
        }
    }
}
