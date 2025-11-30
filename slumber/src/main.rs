//! Slumber - Power management daemon for DaemonOS
//!
//! Provides:
//! - Power profile management (performance, balanced, powersave)
//! - Battery monitoring and thresholds
//! - Suspend/hibernate/hybrid sleep
//! - Idle timeout management

mod battery;
mod config;
mod ipc;
mod profiles;
mod sleep;

use crate::battery::{BatteryMonitor, PowerStatus};
use crate::config::SlumberConfig;
use crate::ipc::{DaemonStatus, IpcHandler, IpcServer};
use crate::profiles::{ProfileManager, ProfileStatus};
use crate::sleep::{SleepManager, SleepStatus};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

/// Slumber - Power management daemon
#[derive(Parser, Debug)]
#[command(name = "slumberd", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/slumber.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/slumber/slumber.sock")]
    socket: PathBuf,

    /// Debug mode
    #[arg(short, long)]
    debug: bool,
}

/// Daemon state
struct SlumberState {
    config: SlumberConfig,
    battery_monitor: RwLock<BatteryMonitor>,
    profile_manager: RwLock<ProfileManager>,
    sleep_manager: SleepManager,
}

impl SlumberState {
    fn new(config: SlumberConfig) -> Self {
        Self {
            battery_monitor: RwLock::new(BatteryMonitor::new(config.battery.clone())),
            profile_manager: RwLock::new(ProfileManager::new(config.profiles.clone())),
            sleep_manager: SleepManager::new(config.sleep.clone()),
            config,
        }
    }
}

impl IpcHandler for SlumberState {
    fn get_power_status(&self) -> Result<PowerStatus> {
        self.battery_monitor.write().unwrap().get_status()
    }

    fn get_profile(&self) -> ProfileStatus {
        self.profile_manager.read().unwrap().get_status()
    }

    fn set_profile(&self, name: &str) -> Result<()> {
        self.profile_manager.write().unwrap().set_profile(name)
    }

    fn list_profiles(&self) -> Vec<String> {
        self.profile_manager
            .read()
            .unwrap()
            .list_profiles()
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }

    fn get_sleep_status(&self) -> SleepStatus {
        self.sleep_manager.get_status()
    }

    fn suspend(&self) -> Result<()> {
        self.sleep_manager.suspend()
    }

    fn hibernate(&self) -> Result<()> {
        self.sleep_manager.hibernate()
    }

    fn hybrid_sleep(&self) -> Result<()> {
        self.sleep_manager.hybrid_sleep()
    }

    fn get_daemon_status(&self) -> Result<DaemonStatus> {
        Ok(DaemonStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            power: self.get_power_status()?,
            profile: self.get_profile(),
            sleep: self.get_sleep_status(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Slumber v{} starting", env!("CARGO_PKG_VERSION"));

    let config = SlumberConfig::load(&args.config)?;
    let state = Arc::new(SlumberState::new(config.clone()));

    // Apply default profile on startup
    if let Err(e) = state.profile_manager.write().unwrap().set_profile(&config.profiles.default_profile) {
        warn!("Failed to apply default profile: {}", e);
    }

    // Start battery monitoring task
    let battery_state = Arc::clone(&state);
    let battery_interval = config.battery.poll_interval_secs;
    tokio::spawn(async move {
        battery_monitor_loop(battery_state, battery_interval).await;
    });

    // Start IPC server
    let socket_path = args.socket.to_string_lossy().to_string();
    let server = IpcServer::new(socket_path, Arc::try_unwrap(state).unwrap_or_else(|arc| (*arc).clone()));

    info!("Slumber ready");
    server.run().await
}

impl Clone for SlumberState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            battery_monitor: RwLock::new(BatteryMonitor::new(self.config.battery.clone())),
            profile_manager: RwLock::new(ProfileManager::new(self.config.profiles.clone())),
            sleep_manager: SleepManager::new(self.config.sleep.clone()),
        }
    }
}

async fn battery_monitor_loop(state: Arc<SlumberState>, interval_secs: u32) {
    use tokio::time::{interval, Duration};

    let mut interval = interval(Duration::from_secs(interval_secs as u64));

    loop {
        interval.tick().await;

        let status = match state.battery_monitor.write().unwrap().get_status() {
            Ok(s) => s,
            Err(e) => {
                warn!("Battery status error: {}", e);
                continue;
            }
        };

        // Check for power source change (auto-switch profiles)
        if state.config.battery.auto_powersave {
            let monitor = state.battery_monitor.read().unwrap();
            if monitor.power_source_changed(&status) {
                let profile = if status.on_ac_power {
                    &state.config.profiles.default_profile
                } else {
                    "powersave"
                };

                drop(monitor);
                if let Err(e) = state.profile_manager.write().unwrap().set_profile(profile) {
                    warn!("Auto profile switch failed: {}", e);
                }
            }
        }

        // Check battery thresholds
        if let Some(action) = state.battery_monitor.read().unwrap().check_thresholds(&status) {
            info!("Battery threshold reached, action: {:?}", action);
            match action {
                config::BatteryAction::Suspend => {
                    let _ = state.sleep_manager.suspend();
                }
                config::BatteryAction::Hibernate => {
                    let _ = state.sleep_manager.hibernate();
                }
                config::BatteryAction::HybridSleep => {
                    let _ = state.sleep_manager.hybrid_sleep();
                }
                _ => {}
            }
        }
    }
}
