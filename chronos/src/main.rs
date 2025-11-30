//! Chronos - Time and NTP daemon for DaemonOS
//!
//! Provides:
//! - NTP time synchronization
//! - System clock management
//! - Timezone handling
//! - RTC synchronization

mod clock;
mod config;
mod ipc;
mod ntp;
mod timezone;

use crate::clock::ClockManager;
use crate::config::ChronosConfig;
use crate::ipc::{
    DaemonStatus, IpcHandler, IpcRequest, IpcResponse, IpcServer, NtpStatus, TimeStatus,
};
use crate::ntp::{NtpClient, SyncState};
use crate::timezone::TimezoneManager;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Chronos - Time and NTP daemon
#[derive(Parser, Debug)]
#[command(name = "chronosd", version, about)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "/etc/chronos/chronos.toml")]
    config: PathBuf,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

/// Daemon state
struct ChronosState {
    config: ChronosConfig,
    ntp_client: NtpClient,
    clock: ClockManager,
    timezone: TimezoneManager,
    sync_state: SyncState,
}

impl ChronosState {
    fn new(config: ChronosConfig) -> Result<Self> {
        let ntp_client = NtpClient::new(config.ntp.clone());
        let clock = ClockManager::new(config.rtc.clone());
        let timezone = TimezoneManager::new(config.timezone.clone())?;

        Ok(Self {
            config,
            ntp_client,
            clock,
            timezone,
            sync_state: SyncState::default(),
        })
    }

    /// Perform NTP synchronization
    fn sync_ntp(&mut self) -> Result<()> {
        if !self.config.ntp.enabled {
            return Ok(());
        }

        match self.ntp_client.sync() {
            Ok(measurement) => {
                // Apply time correction
                self.clock.apply_correction(
                    measurement.offset,
                    self.config.ntp.step_threshold,
                )?;

                // Update sync state
                self.sync_state.update(&measurement);

                info!(
                    "NTP sync successful: offset={:.6}s server={}",
                    measurement.offset, measurement.server
                );

                Ok(())
            }
            Err(e) => {
                self.sync_state.fail();
                Err(e)
            }
        }
    }

    /// Get current time status
    fn get_time_status(&self) -> TimeStatus {
        let now = chrono::Utc::now();
        let local = self.timezone.now();

        TimeStatus {
            utc: now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            local: local.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
            unix_timestamp: self.clock.get_unix_time(),
            timezone: self.timezone.current_name().to_string(),
            utc_offset: self.timezone.utc_offset_string(),
            ntp_synchronized: self.sync_state.synchronized,
        }
    }

    /// Get full daemon status
    fn get_daemon_status(&self) -> DaemonStatus {
        DaemonStatus {
            version: VERSION.to_string(),
            time: self.get_time_status(),
            ntp: NtpStatus::from(&self.sync_state),
            clock: self.clock.get_status(),
            timezone: self.timezone.get_info(),
        }
    }
}

/// IPC handler implementation
struct ChronosHandler {
    state: Arc<RwLock<ChronosState>>,
}

impl IpcHandler for ChronosHandler {
    async fn handle(&self, request: IpcRequest) -> IpcResponse {
        match request {
            IpcRequest::GetStatus => {
                let state = self.state.read().await;
                IpcResponse::success(state.get_time_status())
            }

            IpcRequest::GetSyncStatus => {
                let state = self.state.read().await;
                IpcResponse::success(NtpStatus::from(&state.sync_state))
            }

            IpcRequest::ForceSync => {
                let mut state = self.state.write().await;
                match state.sync_ntp() {
                    Ok(()) => IpcResponse::success(NtpStatus::from(&state.sync_state)),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }

            IpcRequest::GetTimezone => {
                let state = self.state.read().await;
                IpcResponse::success(state.timezone.get_info())
            }

            IpcRequest::SetTimezone { timezone } => {
                let mut state = self.state.write().await;
                match state.timezone.set_timezone(&timezone) {
                    Ok(()) => IpcResponse::success(state.timezone.get_info()),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }

            IpcRequest::ListTimezones { region } => {
                let state = self.state.read().await;
                let timezones = match region {
                    Some(r) => state.timezone.list_timezones_by_region(&r),
                    None => state.timezone.list_timezones(),
                };
                IpcResponse::success(timezones)
            }

            IpcRequest::GetClockStatus => {
                let state = self.state.read().await;
                IpcResponse::success(state.clock.get_status())
            }

            IpcRequest::SyncRtc => {
                let state = self.state.read().await;
                match state.clock.sync_rtc() {
                    Ok(()) => IpcResponse::success(serde_json::json!({"synced": true})),
                    Err(e) => IpcResponse::error(e.to_string()),
                }
            }

            IpcRequest::GetDaemonStatus => {
                let state = self.state.read().await;
                IpcResponse::success(state.get_daemon_status())
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Chronos v{} starting", VERSION);

    // Load configuration
    let config = ChronosConfig::load(&args.config)?;
    info!("Configuration loaded from {:?}", args.config);

    // Initialize state
    let state = Arc::new(RwLock::new(ChronosState::new(config.clone())?));

    // Perform initial NTP sync
    {
        let mut state = state.write().await;
        if let Err(e) = state.sync_ntp() {
            warn!("Initial NTP sync failed: {}", e);
        }
    }

    // Start NTP sync task
    let sync_state = state.clone();
    let poll_interval = config.ntp.poll_interval;
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(poll_interval as u64));

        loop {
            interval.tick().await;

            let mut state = sync_state.write().await;
            if let Err(e) = state.sync_ntp() {
                warn!("NTP sync failed: {}", e);
            }
        }
    });

    // Start RTC sync task if configured
    if config.rtc.enabled && config.rtc.sync_interval > 0 {
        let rtc_state = state.clone();
        let rtc_interval = config.rtc.sync_interval;
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(rtc_interval));

            loop {
                interval.tick().await;

                let state = rtc_state.read().await;
                if let Err(e) = state.clock.sync_rtc() {
                    warn!("RTC sync failed: {}", e);
                }
            }
        });
    }

    // Create IPC handler
    let handler = ChronosHandler {
        state: state.clone(),
    };

    // Start IPC server
    let server = IpcServer::new(&config.daemon.socket_path, handler);

    info!("Chronos ready");
    server.run().await
}
