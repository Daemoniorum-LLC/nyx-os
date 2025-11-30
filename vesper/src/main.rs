//! # Vesper
//!
//! Audio daemon for DaemonOS with PipeWire-compatible interface.
//!
//! ## Features
//!
//! - **ALSA Backend**: Direct hardware access
//! - **PulseAudio Compatibility**: PA protocol support
//! - **Per-App Volume**: Application-level mixing
//! - **Bluetooth Audio**: A2DP/HFP support (via BlueZ)
//! - **Network Audio**: RTP streaming
//! - **Sample Rate Conversion**: High-quality resampling

mod config;
mod device;
mod stream;
mod mixer;
mod sink;
mod source;
mod client;
mod bluetooth;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use libnyx_platform::Platform;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Vesper - Audio Daemon
#[derive(Parser, Debug)]
#[command(name = "vesper", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/vesper.yaml")]
    config: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/vesper/vesper.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List audio devices
    Devices,
    /// List active streams
    Streams,
    /// Set volume
    Volume {
        /// Target (sink name or stream ID)
        target: String,
        /// Volume (0-100 or +/-amount)
        volume: String,
    },
    /// Mute/unmute
    Mute {
        /// Target (sink name or stream ID)
        target: String,
        /// Mute state (on/off/toggle)
        #[arg(default_value = "toggle")]
        state: String,
    },
    /// Set default sink
    SetSink { name: String },
    /// Set default source
    SetSource { name: String },
    /// Show status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    let platform = Platform::detect();

    info!(
        "Vesper v{} starting on {}",
        env!("CARGO_PKG_VERSION"),
        platform.name()
    );

    // Handle CLI commands
    if let Some(cmd) = args.command {
        return handle_client_command(&args.socket, cmd).await;
    }

    // Daemon mode
    run_daemon(args).await
}

async fn handle_client_command(socket: &PathBuf, cmd: Commands) -> Result<()> {
    let client = ipc::VesperClient::new(socket.clone());

    match cmd {
        Commands::Devices => {
            let devices = client.list_devices().await?;
            println!("{:<30} {:<10} {:<10}", "NAME", "TYPE", "STATE");
            println!("{}", "-".repeat(55));
            for dev in devices {
                println!(
                    "{:<30} {:<10} {:<10}",
                    dev.name, dev.device_type, dev.state
                );
            }
        }
        Commands::Streams => {
            let streams = client.list_streams().await?;
            println!("{:<8} {:<20} {:<15} {:<8}", "ID", "APP", "SINK", "VOLUME");
            println!("{}", "-".repeat(55));
            for stream in streams {
                println!(
                    "{:<8} {:<20} {:<15} {:>5}%",
                    stream.id, stream.app_name, stream.sink, stream.volume
                );
            }
        }
        Commands::Volume { target, volume } => {
            client.set_volume(&target, &volume).await?;
            println!("Volume set");
        }
        Commands::Mute { target, state } => {
            let muted = client.set_mute(&target, &state).await?;
            println!("Muted: {}", muted);
        }
        Commands::SetSink { name } => {
            client.set_default_sink(&name).await?;
            println!("Default sink set to {}", name);
        }
        Commands::SetSource { name } => {
            client.set_default_source(&name).await?;
            println!("Default source set to {}", name);
        }
        Commands::Status => {
            let status = client.get_status().await?;
            println!("Default Sink:   {}", status.default_sink);
            println!("Default Source: {}", status.default_source);
            println!("Active Streams: {}", status.stream_count);
            println!("Master Volume:  {}%", status.master_volume);
            println!("Muted:          {}", status.muted);
        }
    }

    Ok(())
}

async fn run_daemon(args: Args) -> Result<()> {
    // Ensure runtime directory
    std::fs::create_dir_all("/run/vesper")?;

    // Load configuration
    let config = config::load_config(&args.config)?;

    // Initialize device manager
    let device_manager = Arc::new(RwLock::new(device::DeviceManager::new()?));

    // Enumerate devices
    {
        let mut dm = device_manager.write().await;
        dm.enumerate()?;
        info!("Found {} audio devices", dm.device_count());
    }

    // Initialize mixer
    let mixer = Arc::new(RwLock::new(mixer::Mixer::new(config.clone())));

    // Initialize sinks
    let sinks = Arc::new(RwLock::new(HashMap::new()));
    {
        let dm = device_manager.read().await;
        let mut sink_map = sinks.write().await;

        for dev in dm.playback_devices() {
            let sink = sink::Sink::new(dev.clone(), config.clone())?;
            sink_map.insert(dev.name.clone(), sink);
        }

        if !sink_map.is_empty() {
            info!("Initialized {} sinks", sink_map.len());
        }
    }

    // Initialize sources
    let sources = Arc::new(RwLock::new(HashMap::new()));
    {
        let dm = device_manager.read().await;
        let mut source_map = sources.write().await;

        for dev in dm.capture_devices() {
            let source = source::Source::new(dev.clone(), config.clone())?;
            source_map.insert(dev.name.clone(), source);
        }

        if !source_map.is_empty() {
            info!("Initialized {} sources", source_map.len());
        }
    }

    // Initialize client manager
    let clients = Arc::new(RwLock::new(client::ClientManager::new()));

    // Initialize Bluetooth if available
    let bluetooth = if config.bluetooth_enabled {
        match bluetooth::BluetoothAudio::new() {
            Ok(bt) => {
                info!("Bluetooth audio enabled");
                Some(Arc::new(RwLock::new(bt)))
            }
            Err(e) => {
                warn!("Bluetooth audio not available: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Start audio processing
    let audio_context = AudioContext {
        device_manager,
        mixer,
        sinks,
        sources,
        clients,
        bluetooth,
        config: config.clone(),
    };

    // Start IPC server
    let server = ipc::VesperServer::new(args.socket.clone(), audio_context);

    info!("Vesper ready on {:?}", args.socket);
    server.run().await
}

/// Shared audio context
pub struct AudioContext {
    pub device_manager: Arc<RwLock<device::DeviceManager>>,
    pub mixer: Arc<RwLock<mixer::Mixer>>,
    pub sinks: Arc<RwLock<HashMap<String, sink::Sink>>>,
    pub sources: Arc<RwLock<HashMap<String, source::Source>>>,
    pub clients: Arc<RwLock<client::ClientManager>>,
    pub bluetooth: Option<Arc<RwLock<bluetooth::BluetoothAudio>>>,
    pub config: config::Config,
}
