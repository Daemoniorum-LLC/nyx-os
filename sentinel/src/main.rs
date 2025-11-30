//! Sentinel - System monitoring daemon for DaemonOS
//!
//! Provides:
//! - CPU, memory, disk, network metrics
//! - Temperature monitoring
//! - Process tracking
//! - Alert management
//! - Metrics history

mod alerts;
mod config;
mod ipc;
mod metrics;

use crate::alerts::{Alert, AlertManager};
use crate::config::SentinelConfig;
use crate::ipc::{DaemonStatus, IpcHandler, IpcServer};
use crate::metrics::{MetricsCollector, SystemSnapshot};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::info;

/// Sentinel - System monitoring daemon
#[derive(Parser, Debug)]
#[command(name = "sentineld", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/sentinel.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/sentinel/sentinel.sock")]
    socket: PathBuf,

    /// Debug mode
    #[arg(short, long)]
    debug: bool,
}

/// Daemon state
struct SentinelState {
    config: SentinelConfig,
    collector: RwLock<MetricsCollector>,
    alerts: RwLock<AlertManager>,
    start_time: Instant,
}

impl SentinelState {
    fn new(config: SentinelConfig) -> Self {
        Self {
            collector: RwLock::new(MetricsCollector::new(config.metrics.clone())),
            alerts: RwLock::new(AlertManager::new(config.alerts.clone())),
            start_time: Instant::now(),
            config,
        }
    }
}

impl IpcHandler for SentinelState {
    fn get_metrics(&self) -> Option<SystemSnapshot> {
        self.collector.read().unwrap().latest().cloned()
    }

    fn get_history(&self, limit: usize) -> Vec<SystemSnapshot> {
        self.collector
            .read()
            .unwrap()
            .get_history()
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    fn get_alerts(&self) -> Vec<Alert> {
        self.alerts
            .read()
            .unwrap()
            .get_active_alerts()
            .into_iter()
            .cloned()
            .collect()
    }

    fn get_alert_history(&self, limit: usize) -> Vec<Alert> {
        self.alerts
            .read()
            .unwrap()
            .get_history(limit)
            .into_iter()
            .cloned()
            .collect()
    }

    fn get_status(&self) -> DaemonStatus {
        DaemonStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            collection_interval: self.config.metrics.interval_secs,
            history_size: self.collector.read().unwrap().get_history().len(),
            alerts: self.alerts.read().unwrap().get_counts(),
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

    info!("Sentinel v{} starting", env!("CARGO_PKG_VERSION"));

    let config = SentinelConfig::load(&args.config)?;
    let state = Arc::new(SentinelState::new(config.clone()));

    // Start metrics collection task
    let collection_state = Arc::clone(&state);
    let interval = config.metrics.interval_secs;
    tokio::spawn(async move {
        collection_loop(collection_state, interval).await;
    });

    // Start IPC server
    let socket_path = args.socket.to_string_lossy().to_string();
    let server = IpcServer::new(socket_path, Arc::try_unwrap(state).unwrap_or_else(|arc| (*arc).clone()));

    info!("Sentinel ready");
    server.run().await
}

impl Clone for SentinelState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            collector: RwLock::new(MetricsCollector::new(self.config.metrics.clone())),
            alerts: RwLock::new(AlertManager::new(self.config.alerts.clone())),
            start_time: self.start_time,
        }
    }
}

async fn collection_loop(state: Arc<SentinelState>, interval_secs: u32) {
    use tokio::time::{interval, Duration};

    let mut interval = interval(Duration::from_secs(interval_secs as u64));

    loop {
        interval.tick().await;

        // Collect metrics
        let snapshot = state.collector.write().unwrap().collect();

        // Check for alerts
        let _new_alerts = state.alerts.write().unwrap().check(&snapshot);

        // Could emit alerts to a notification service here
    }
}
