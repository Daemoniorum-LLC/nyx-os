//! # Nyx Service Manager (nyx-serviced)
//!
//! A systemd-like service manager for DaemonOS with platform-aware capabilities.
//!
//! ## Features
//!
//! - **Unit Files**: YAML/TOML service definitions
//! - **Dependencies**: Before/After/Requires/Wants semantics
//! - **Socket Activation**: On-demand service startup
//! - **Resource Limits**: Cgroups v2 integration (platform-aware)
//! - **Restart Policies**: Always, OnFailure, Never
//! - **Watchdog**: Health monitoring and auto-restart
//! - **IPC**: Unix socket control interface

mod unit;
mod state;
mod dependency;
mod lifecycle;
mod socket_activation;
mod cgroups;
mod watchdog;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use libnyx_platform::{Platform, PlatformCapabilities};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Nyx Service Manager
#[derive(Parser, Debug)]
#[command(name = "nyx-serviced", version, about)]
struct Args {
    /// Configuration directory
    #[arg(short, long, default_value = "/grimoire/services")]
    config_dir: PathBuf,

    /// Runtime directory for sockets/pids
    #[arg(short, long, default_value = "/run/nyx")]
    runtime_dir: PathBuf,

    /// Control socket path
    #[arg(short, long, default_value = "/run/nyx/serviced.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a service
    Start { name: String },
    /// Stop a service
    Stop { name: String },
    /// Restart a service
    Restart { name: String },
    /// Reload service configuration
    Reload { name: String },
    /// Get service status
    Status { name: Option<String> },
    /// Enable service to start at boot
    Enable { name: String },
    /// Disable service from starting at boot
    Disable { name: String },
    /// List all services
    List {
        /// Show only running services
        #[arg(long)]
        running: bool,
    },
    /// Show service logs
    Logs {
        name: String,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    let platform = Platform::detect();
    let capabilities = PlatformCapabilities::detect();

    info!(
        "nyx-serviced v{} on {} (cgroups_v2={}, systemd={})",
        env!("CARGO_PKG_VERSION"),
        platform.name(),
        capabilities.cgroups_v2,
        capabilities.systemd
    );

    // If systemd is available on this platform, warn about potential conflicts
    if capabilities.systemd {
        warn!("systemd detected - nyx-serviced running in parallel mode");
    }

    // Handle CLI commands
    if let Some(cmd) = args.command {
        return handle_client_command(&args.socket, cmd).await;
    }

    // Daemon mode
    run_daemon(args, capabilities).await
}

async fn handle_client_command(socket: &PathBuf, cmd: Commands) -> Result<()> {
    let client = ipc::ServicedClient::new(socket.clone());

    match cmd {
        Commands::Start { name } => {
            let result = client.start(&name).await?;
            println!("{}", result);
        }
        Commands::Stop { name } => {
            let result = client.stop(&name).await?;
            println!("{}", result);
        }
        Commands::Restart { name } => {
            let result = client.restart(&name).await?;
            println!("{}", result);
        }
        Commands::Reload { name } => {
            let result = client.reload(&name).await?;
            println!("{}", result);
        }
        Commands::Status { name } => {
            let status = client.status(name.as_deref()).await?;
            print_status(&status);
        }
        Commands::Enable { name } => {
            let result = client.enable(&name).await?;
            println!("{}", result);
        }
        Commands::Disable { name } => {
            let result = client.disable(&name).await?;
            println!("{}", result);
        }
        Commands::List { running } => {
            let services = client.list(running).await?;
            print_list(&services);
        }
        Commands::Logs { name, follow, lines } => {
            if follow {
                client.follow_logs(&name).await?;
            } else {
                let logs = client.logs(&name, lines).await?;
                for line in logs {
                    println!("{}", line);
                }
            }
        }
    }

    Ok(())
}

fn print_status(status: &ipc::ServiceStatus) {
    let state_color = match status.state.as_str() {
        "running" => "\x1b[32m", // green
        "stopped" => "\x1b[90m", // gray
        "failed" => "\x1b[31m",  // red
        "starting" => "\x1b[33m", // yellow
        _ => "\x1b[0m",
    };

    println!("● {}", status.name);
    println!("   State: {}{}\x1b[0m", state_color, status.state);
    if let Some(pid) = status.pid {
        println!("   PID: {}", pid);
    }
    if let Some(started) = &status.started_at {
        println!("   Started: {}", started);
    }
    if let Some(uptime) = &status.uptime {
        println!("   Uptime: {}", uptime);
    }
    if let Some(memory) = status.memory_bytes {
        println!("   Memory: {} MB", memory / 1024 / 1024);
    }
    if let Some(restarts) = status.restart_count {
        println!("   Restarts: {}", restarts);
    }
    if let Some(exit_code) = status.last_exit_code {
        println!("   Last Exit: {}", exit_code);
    }
}

fn print_list(services: &[ipc::ServiceListEntry]) {
    println!("{:<20} {:<10} {:<8} {:<20}", "SERVICE", "STATE", "PID", "UPTIME");
    println!("{}", "-".repeat(60));

    for svc in services {
        let state_color = match svc.state.as_str() {
            "running" => "\x1b[32m",
            "stopped" => "\x1b[90m",
            "failed" => "\x1b[31m",
            _ => "\x1b[0m",
        };

        println!(
            "{:<20} {}{:<10}\x1b[0m {:<8} {:<20}",
            svc.name,
            state_color,
            svc.state,
            svc.pid.map(|p| p.to_string()).unwrap_or("-".into()),
            svc.uptime.as_deref().unwrap_or("-")
        );
    }
}

async fn run_daemon(args: Args, capabilities: PlatformCapabilities) -> Result<()> {
    // Ensure runtime directory exists
    std::fs::create_dir_all(&args.runtime_dir)?;

    // Initialize components
    let unit_registry = Arc::new(RwLock::new(unit::UnitRegistry::new()));
    let state_manager = Arc::new(RwLock::new(state::StateManager::new()));
    let lifecycle = Arc::new(lifecycle::LifecycleManager::new(
        unit_registry.clone(),
        state_manager.clone(),
        capabilities.clone(),
    ));

    // Initialize cgroups if available
    let cgroup_manager = if capabilities.cgroups_v2 {
        info!("Initializing cgroups v2 resource manager");
        Some(Arc::new(cgroups::CgroupManager::new()?))
    } else {
        warn!("Cgroups v2 not available - resource limits disabled");
        None
    };

    // Load unit files
    info!("Loading service units from {:?}", args.config_dir);
    let loaded = unit_registry.write().await.load_directory(&args.config_dir)?;
    info!("Loaded {} service units", loaded);

    // Resolve dependencies
    {
        let registry = unit_registry.read().await;
        let units: Vec<_> = registry.all().collect();
        let order = dependency::resolve_order(&units)?;
        info!("Dependency order: {:?}", order.iter().map(|u| &u.name).collect::<Vec<_>>());
    }

    // Start enabled services
    let enabled_count = start_enabled_services(&lifecycle).await?;
    info!("Started {} enabled services", enabled_count);

    // Initialize socket activation
    let socket_activator = Arc::new(socket_activation::SocketActivator::new(
        lifecycle.clone(),
        args.runtime_dir.clone(),
    ));
    socket_activator.setup_sockets(&*unit_registry.read().await).await?;

    // Start watchdog
    let watchdog = Arc::new(watchdog::Watchdog::new(
        lifecycle.clone(),
        state_manager.clone(),
    ));
    tokio::spawn({
        let wd = watchdog.clone();
        async move {
            wd.run().await;
        }
    });

    // Start IPC server
    let server = ipc::ServicedServer::new(
        args.socket.clone(),
        lifecycle.clone(),
        state_manager.clone(),
        unit_registry.clone(),
    );

    info!("nyx-serviced ready on {:?}", args.socket);
    server.run().await
}

async fn start_enabled_services(lifecycle: &Arc<lifecycle::LifecycleManager>) -> Result<usize> {
    lifecycle.start_enabled().await
}
