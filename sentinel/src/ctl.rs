//! sentinelctl - Sentinel control utility

mod alerts;
mod config;
mod ipc;
mod metrics;

use crate::ipc::{IpcClient, IpcRequest};
use anyhow::Result;
use clap::{Parser, Subcommand};

/// Sentinel control utility
#[derive(Parser)]
#[command(name = "sentinelctl", version, about = "Control the Sentinel monitoring daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/sentinel/sentinel.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system overview
    Status,

    /// Show CPU metrics
    Cpu,

    /// Show memory metrics
    Memory,

    /// Show disk metrics
    Disks,

    /// Show network metrics
    Networks,

    /// Show temperature sensors
    Temps,

    /// Show load average
    Load,

    /// Show system uptime
    Uptime,

    /// Show top processes
    Top {
        /// Sort by cpu or memory
        #[arg(short, long, default_value = "cpu")]
        sort: String,
    },

    /// Show active alerts
    Alerts,

    /// Show full daemon info
    Info,
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = IpcClient::new(&cli.socket);

    match cli.command {
        Commands::Status => {
            let metrics = client.get_metrics().await?;
            let alerts = client.get_alerts().await?;

            println!("System Status");
            println!("=============");

            if let Some(cpu) = &metrics.cpu {
                println!("CPU:       {:.1}% ({} cores)", cpu.usage, cpu.logical_cores);
            }

            if let Some(mem) = &metrics.memory {
                println!(
                    "Memory:    {:.1}% ({} / {})",
                    mem.usage_percent,
                    format_bytes(mem.used),
                    format_bytes(mem.total)
                );
            }

            println!(
                "Load:      {:.2} / {:.2} / {:.2}",
                metrics.load.one, metrics.load.five, metrics.load.fifteen
            );

            println!(
                "Uptime:    {}d {}h {}m",
                metrics.uptime.days, metrics.uptime.hours, metrics.uptime.minutes
            );

            if !alerts.is_empty() {
                println!();
                println!("Active Alerts: {}", alerts.len());
                for alert in &alerts {
                    println!("  [{:?}] {}", alert.severity, alert.message);
                }
            }
        }

        Commands::Cpu => {
            let metrics = client.get_metrics().await?;

            if let Some(cpu) = metrics.cpu {
                println!("CPU Information");
                println!("===============");
                println!("Model:     {}", cpu.brand);
                println!("Cores:     {} physical, {} logical", cpu.physical_cores, cpu.logical_cores);
                println!("Frequency: {} MHz", cpu.frequency);
                println!("Usage:     {:.1}%", cpu.usage);
                println!();
                println!("Per-core Usage:");
                for (i, usage) in cpu.cores.iter().enumerate() {
                    let bar = "█".repeat((usage / 5.0) as usize);
                    println!("  Core {:2}: {:5.1}% {}", i, usage, bar);
                }
            } else {
                println!("CPU metrics not available");
            }
        }

        Commands::Memory => {
            let metrics = client.get_metrics().await?;

            if let Some(mem) = metrics.memory {
                println!("Memory Information");
                println!("==================");
                println!("Total:     {}", format_bytes(mem.total));
                println!("Used:      {}", format_bytes(mem.used));
                println!("Free:      {}", format_bytes(mem.free));
                println!("Available: {}", format_bytes(mem.available));
                println!("Usage:     {:.1}%", mem.usage_percent);
                println!();
                println!("Swap:");
                println!("  Total:   {}", format_bytes(mem.swap_total));
                println!("  Used:    {}", format_bytes(mem.swap_used));
                println!("  Free:    {}", format_bytes(mem.swap_free));
            } else {
                println!("Memory metrics not available");
            }
        }

        Commands::Disks => {
            let metrics = client.get_metrics().await?;

            println!("Disk Information");
            println!("================");

            if metrics.disks.is_empty() {
                println!("No disks found");
            } else {
                for disk in &metrics.disks {
                    println!("{} ({})", disk.mount_point, disk.fs_type);
                    println!(
                        "  Size:  {} / {} ({:.1}% used)",
                        format_bytes(disk.used),
                        format_bytes(disk.total),
                        disk.usage_percent
                    );
                    let bar_width = 30;
                    let filled = (disk.usage_percent / 100.0 * bar_width as f32) as usize;
                    let bar = format!(
                        "[{}{}]",
                        "█".repeat(filled),
                        "░".repeat(bar_width - filled)
                    );
                    println!("  {}", bar);
                    println!();
                }
            }
        }

        Commands::Networks => {
            let metrics = client.get_metrics().await?;

            println!("Network Interfaces");
            println!("==================");

            if metrics.networks.is_empty() {
                println!("No network interfaces found");
            } else {
                for net in &metrics.networks {
                    println!("{}:", net.name);
                    println!("  RX: {} ({} packets)", format_bytes(net.rx_bytes), net.rx_packets);
                    println!("  TX: {} ({} packets)", format_bytes(net.tx_bytes), net.tx_packets);
                    if net.rx_errors > 0 || net.tx_errors > 0 {
                        println!("  Errors: {} RX, {} TX", net.rx_errors, net.tx_errors);
                    }
                    println!();
                }
            }
        }

        Commands::Temps => {
            let metrics = client.get_metrics().await?;

            println!("Temperature Sensors");
            println!("===================");

            if metrics.temperatures.is_empty() {
                println!("No temperature sensors found");
            } else {
                for temp in &metrics.temperatures {
                    let critical = temp
                        .critical
                        .map(|c| format!(" (critical: {:.0}°C)", c))
                        .unwrap_or_default();
                    println!("{}: {:.1}°C{}", temp.label, temp.temperature, critical);
                }
            }
        }

        Commands::Load => {
            let metrics = client.get_metrics().await?;

            println!("Load Average");
            println!("============");
            println!("1 min:   {:.2}", metrics.load.one);
            println!("5 min:   {:.2}", metrics.load.five);
            println!("15 min:  {:.2}", metrics.load.fifteen);

            if let Some(cpu) = &metrics.cpu {
                let per_core = metrics.load.one / cpu.logical_cores as f64;
                println!();
                println!("Per core (1 min): {:.2}", per_core);
            }
        }

        Commands::Uptime => {
            let metrics = client.get_metrics().await?;

            println!("System Uptime");
            println!("=============");
            println!(
                "{}d {}h {}m ({}s total)",
                metrics.uptime.days,
                metrics.uptime.hours,
                metrics.uptime.minutes,
                metrics.uptime.seconds
            );
        }

        Commands::Top { sort } => {
            let metrics = client.get_metrics().await?;

            let processes = if sort == "memory" {
                &metrics.top_memory_processes
            } else {
                &metrics.top_cpu_processes
            };

            println!("Top Processes by {}", if sort == "memory" { "Memory" } else { "CPU" });
            println!("============================");
            println!("{:>7} {:>6} {:>10} {}", "PID", "CPU%", "MEM", "NAME");

            for proc in processes {
                println!(
                    "{:>7} {:>5.1}% {:>10} {}",
                    proc.pid,
                    proc.cpu_usage,
                    format_bytes(proc.memory),
                    proc.name
                );
            }
        }

        Commands::Alerts => {
            let alerts = client.get_alerts().await?;

            println!("Active Alerts");
            println!("=============");

            if alerts.is_empty() {
                println!("No active alerts");
            } else {
                for alert in &alerts {
                    let resource = alert
                        .resource
                        .as_ref()
                        .map(|r| format!(" ({})", r))
                        .unwrap_or_default();
                    println!(
                        "[{:?}] {:?}{}: {} (value: {:.1}, threshold: {:.1})",
                        alert.severity,
                        alert.alert_type,
                        resource,
                        alert.message,
                        alert.value,
                        alert.threshold
                    );
                }
            }
        }

        Commands::Info => {
            let status = client.get_status().await?;

            println!("Sentinel Daemon Status");
            println!("======================");
            println!("Version:           {}", status.version);
            println!("Daemon uptime:     {}s", status.uptime_secs);
            println!("Collection interval: {}s", status.collection_interval);
            println!("History samples:   {}", status.history_size);
            println!();
            println!("Alerts:");
            println!("  Critical: {}", status.alerts.critical);
            println!("  Warning:  {}", status.alerts.warning);
            println!("  Info:     {}", status.alerts.info);
        }
    }

    Ok(())
}
