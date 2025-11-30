//! # Phantom
//!
//! Device manager for DaemonOS - a udev-like device management daemon.
//!
//! ## Features
//!
//! - **Device Enumeration**: Scan /sys for devices
//! - **Hotplug Detection**: Netlink socket monitoring
//! - **Rule Processing**: Match and action rules
//! - **Device Nodes**: Automatic /dev node creation
//! - **Device Properties**: Sysfs attribute reading
//! - **Permissions**: Device node permission management

mod device;
mod rule;
mod netlink;
mod devnode;
mod hwdb;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use libnyx_platform::Platform;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Phantom - Device Manager
#[derive(Parser, Debug)]
#[command(name = "phantom", version, about)]
struct Args {
    /// Rules directory
    #[arg(short, long, default_value = "/grimoire/system/phantom.d")]
    rules_dir: PathBuf,

    /// Socket path
    #[arg(short, long, default_value = "/run/phantom/phantom.sock")]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all devices
    List {
        /// Filter by subsystem
        #[arg(short, long)]
        subsystem: Option<String>,
    },
    /// Show device info
    Info { path: String },
    /// Trigger device events
    Trigger {
        /// Subsystem to trigger
        #[arg(short, long)]
        subsystem: Option<String>,
        /// Action (add, change, remove)
        #[arg(short, long, default_value = "change")]
        action: String,
    },
    /// Monitor device events
    Monitor,
    /// Test rules against a device
    Test { path: String },
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
        "Phantom v{} starting on {}",
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
    let client = ipc::PhantomClient::new(socket.clone());

    match cmd {
        Commands::List { subsystem } => {
            let devices = client.list_devices(subsystem.as_deref()).await?;
            println!("{:<50} {:<15} {:<20}", "PATH", "SUBSYSTEM", "DRIVER");
            println!("{}", "-".repeat(90));
            for dev in devices {
                println!(
                    "{:<50} {:<15} {:<20}",
                    dev.syspath,
                    dev.subsystem.as_deref().unwrap_or("-"),
                    dev.driver.as_deref().unwrap_or("-")
                );
            }
        }
        Commands::Info { path } => {
            let info = client.get_device(&path).await?;
            println!("Path:       {}", info.syspath);
            println!("Subsystem:  {}", info.subsystem.as_deref().unwrap_or("-"));
            println!("Driver:     {}", info.driver.as_deref().unwrap_or("-"));
            println!("Dev Node:   {}", info.devnode.as_deref().unwrap_or("-"));
            if !info.properties.is_empty() {
                println!("Properties:");
                for (key, value) in &info.properties {
                    println!("  {}={}", key, value);
                }
            }
        }
        Commands::Trigger { subsystem, action } => {
            client.trigger(subsystem.as_deref(), &action).await?;
            println!("Trigger sent");
        }
        Commands::Monitor => {
            client.monitor().await?;
        }
        Commands::Test { path } => {
            let result = client.test_rules(&path).await?;
            println!("Rule test results for {}:", path);
            for rule in result {
                println!("  {} -> {}", rule.0, rule.1);
            }
        }
    }

    Ok(())
}

async fn run_daemon(args: Args) -> Result<()> {
    // Ensure runtime directory
    std::fs::create_dir_all("/run/phantom")?;

    // Initialize device database
    let devices = Arc::new(RwLock::new(device::DeviceDatabase::new()));

    // Load rules
    let rules = Arc::new(RwLock::new(rule::RuleSet::new()));
    {
        let mut rule_set = rules.write().await;
        if let Err(e) = rule_set.load_directory(&args.rules_dir) {
            warn!("Failed to load some rules: {}", e);
        }
        info!("Loaded {} rules", rule_set.rule_count());
    }

    // Initial device enumeration
    info!("Enumerating devices...");
    {
        let mut db = devices.write().await;
        let count = db.enumerate()?;
        info!("Found {} devices", count);
    }

    // Process initial devices with rules
    {
        let db = devices.read().await;
        let rule_set = rules.read().await;

        for device in db.all() {
            if let Err(e) = process_device(device, &rule_set, "add").await {
                warn!("Failed to process device {}: {}", device.syspath, e);
            }
        }
    }

    // Start netlink monitor
    let devices_clone = devices.clone();
    let rules_clone = rules.clone();

    let netlink_handle = tokio::spawn(async move {
        if let Err(e) = run_netlink_monitor(devices_clone, rules_clone).await {
            error!("Netlink monitor error: {}", e);
        }
    });

    // Start IPC server
    let server = ipc::PhantomServer::new(
        args.socket.clone(),
        devices.clone(),
        rules.clone(),
    );

    info!("Phantom ready on {:?}", args.socket);

    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!("IPC server error: {}", e);
            }
        }
        _ = netlink_handle => {
            info!("Netlink monitor exited");
        }
    }

    Ok(())
}

async fn run_netlink_monitor(
    devices: Arc<RwLock<device::DeviceDatabase>>,
    rules: Arc<RwLock<rule::RuleSet>>,
) -> Result<()> {
    let mut monitor = netlink::NetlinkMonitor::new()?;

    info!("Netlink monitor started");

    loop {
        match monitor.receive_event().await {
            Ok(Some(event)) => {
                info!(
                    "Device event: {} {} ({})",
                    event.action,
                    event.devpath,
                    event.subsystem.as_deref().unwrap_or("unknown")
                );

                // Update device database
                {
                    let mut db = devices.write().await;

                    match event.action.as_str() {
                        "add" | "change" => {
                            if let Ok(dev) = device::Device::from_syspath(&event.devpath) {
                                db.add(dev);
                            }
                        }
                        "remove" => {
                            db.remove(&event.devpath);
                        }
                        _ => {}
                    }
                }

                // Process rules
                {
                    let db = devices.read().await;
                    let rule_set = rules.read().await;

                    if let Some(device) = db.get(&event.devpath) {
                        if let Err(e) = process_device(device, &rule_set, &event.action).await {
                            warn!("Failed to process device event: {}", e);
                        }
                    }
                }
            }
            Ok(None) => {
                // No event, continue
            }
            Err(e) => {
                error!("Netlink receive error: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

async fn process_device(
    device: &device::Device,
    rules: &rule::RuleSet,
    action: &str,
) -> Result<()> {
    // Find matching rules
    let matched = rules.find_matches(device);

    for rule in matched {
        // Execute rule actions
        for rule_action in &rule.actions {
            if let Err(e) = execute_action(device, rule_action, action).await {
                warn!("Action failed for {}: {}", device.syspath, e);
            }
        }
    }

    Ok(())
}

async fn execute_action(
    device: &device::Device,
    action: &rule::RuleAction,
    event_action: &str,
) -> Result<()> {
    use rule::RuleAction;

    match action {
        RuleAction::Name(name) => {
            if event_action == "add" {
                devnode::create_devnode(device, Some(name)).await?;
            }
        }
        RuleAction::Symlink(link) => {
            if event_action == "add" {
                devnode::create_symlink(device, link).await?;
            } else if event_action == "remove" {
                devnode::remove_symlink(link).await?;
            }
        }
        RuleAction::Mode(mode) => {
            if let Some(devnode) = &device.devnode {
                devnode::set_permissions(devnode, *mode).await?;
            }
        }
        RuleAction::Owner(owner) => {
            if let Some(devnode) = &device.devnode {
                devnode::set_owner(devnode, owner).await?;
            }
        }
        RuleAction::Group(group) => {
            if let Some(devnode) = &device.devnode {
                devnode::set_group(devnode, group).await?;
            }
        }
        RuleAction::Run(cmd) => {
            if event_action == "add" {
                rule::run_program(cmd, device).await?;
            }
        }
        RuleAction::Tag(tag) => {
            // Would tag the device in database
            tracing::debug!("Tagged {} with {}", device.syspath, tag);
        }
        RuleAction::Env(key, value) => {
            // Would set environment for spawned programs
            tracing::debug!("Set env {}={}", key, value);
        }
    }

    Ok(())
}
