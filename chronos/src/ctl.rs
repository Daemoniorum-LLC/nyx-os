//! chronosctl - Chronos control utility

mod clock;
mod config;
mod ipc;
mod ntp;
mod timezone;

use crate::ipc::IpcClient;
use anyhow::Result;
use clap::{Parser, Subcommand};

/// Chronos control utility
#[derive(Parser)]
#[command(name = "chronosctl", version, about = "Control the Chronos time daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/chronos/chronos.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current time status
    Status,

    /// Show NTP synchronization status
    Ntp {
        #[command(subcommand)]
        command: NtpCommands,
    },

    /// Timezone management
    Timezone {
        #[command(subcommand)]
        command: TimezoneCommands,
    },

    /// Clock management
    Clock {
        #[command(subcommand)]
        command: ClockCommands,
    },

    /// Show full daemon status
    Info,
}

#[derive(Subcommand)]
enum NtpCommands {
    /// Show NTP sync status
    Status,

    /// Force NTP synchronization
    Sync,
}

#[derive(Subcommand)]
enum TimezoneCommands {
    /// Show current timezone
    Show,

    /// Set timezone
    Set {
        /// Timezone name (IANA format, e.g., America/New_York)
        timezone: String,
    },

    /// List available timezones
    List {
        /// Filter by region (e.g., America, Europe, Asia)
        #[arg(short, long)]
        region: Option<String>,
    },
}

#[derive(Subcommand)]
enum ClockCommands {
    /// Show clock status
    Status,

    /// Sync RTC from system clock
    SyncRtc,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = IpcClient::new(&cli.socket);

    match cli.command {
        Commands::Status => {
            let status = client.get_status().await?;
            println!("Time Status");
            println!("===========");
            println!("UTC:           {}", status.utc);
            println!("Local:         {}", status.local);
            println!("Unix:          {:.3}", status.unix_timestamp);
            println!("Timezone:      {} ({})", status.timezone, status.utc_offset);
            println!(
                "NTP:           {}",
                if status.ntp_synchronized {
                    "synchronized"
                } else {
                    "not synchronized"
                }
            );
        }

        Commands::Ntp { command } => match command {
            NtpCommands::Status => {
                let status = client.get_sync_status().await?;
                println!("NTP Status");
                println!("==========");
                println!(
                    "Synchronized:  {}",
                    if status.synchronized { "yes" } else { "no" }
                );
                if let Some(ref last_sync) = status.last_sync {
                    println!("Last sync:     {}", last_sync);
                }
                println!("Offset:        {:.6} s", status.last_offset);
                println!("Delay:         {:.6} s", status.last_delay);
                println!("Stratum:       {}", status.stratum);
                if let Some(ref server) = status.ref_server {
                    println!("Server:        {}", server);
                }
                println!("Sync count:    {}", status.sync_count);
                println!("Fail count:    {}", status.fail_count);
            }

            NtpCommands::Sync => {
                println!("Forcing NTP synchronization...");
                let status = client.force_sync().await?;
                if status.synchronized {
                    println!(
                        "Synchronized: offset={:.6}s server={}",
                        status.last_offset,
                        status.ref_server.as_deref().unwrap_or("unknown")
                    );
                } else {
                    println!("Synchronization failed");
                }
            }
        },

        Commands::Timezone { command } => match command {
            TimezoneCommands::Show => {
                let info = client.get_timezone().await?;
                println!("Timezone Information");
                println!("====================");
                println!("Name:          {}", info.name);
                println!("UTC Offset:    {}", info.offset);
                println!("Offset (sec):  {}", info.offset_seconds);
                println!("Has DST:       {}", if info.has_dst { "yes" } else { "no" });
                println!("In DST:        {}", if info.is_dst { "yes" } else { "no" });
            }

            TimezoneCommands::Set { timezone } => {
                let info = client.set_timezone(&timezone).await?;
                println!("Timezone set to: {} ({})", info.name, info.offset);
            }

            TimezoneCommands::List { region } => {
                let timezones = client.list_timezones(region.as_deref()).await?;
                println!("Available Timezones");
                println!("===================");
                for tz in &timezones {
                    println!("  {}", tz);
                }
                println!("\nTotal: {} timezones", timezones.len());
            }
        },

        Commands::Clock { command } => match command {
            ClockCommands::Status => {
                let status = client.get_daemon_status().await?;
                println!("Clock Status");
                println!("============");
                println!("Unix timestamp: {:.6}", status.clock.unix_timestamp);
                println!("Uptime:         {:.1} seconds", status.clock.uptime_secs);
                println!(
                    "RTC:            {}",
                    if status.clock.rtc_available {
                        "available"
                    } else {
                        "not available"
                    }
                );
            }

            ClockCommands::SyncRtc => {
                client.sync_rtc().await?;
                println!("RTC synchronized with system clock");
            }
        },

        Commands::Info => {
            let status = client.get_daemon_status().await?;
            println!("Chronos Daemon Status");
            println!("=====================");
            println!("Version:       {}", status.version);
            println!();
            println!("Time:");
            println!("  UTC:         {}", status.time.utc);
            println!("  Local:       {}", status.time.local);
            println!("  Timezone:    {} ({})", status.time.timezone, status.time.utc_offset);
            println!();
            println!("NTP:");
            println!(
                "  Synchronized: {}",
                if status.ntp.synchronized { "yes" } else { "no" }
            );
            println!("  Offset:      {:.6} s", status.ntp.last_offset);
            println!("  Delay:       {:.6} s", status.ntp.last_delay);
            println!("  Stratum:     {}", status.ntp.stratum);
            if let Some(ref server) = status.ntp.ref_server {
                println!("  Server:      {}", server);
            }
            println!();
            println!("Clock:");
            println!("  Uptime:      {:.1} s", status.clock.uptime_secs);
            println!(
                "  RTC:         {}",
                if status.clock.rtc_available {
                    "available"
                } else {
                    "not available"
                }
            );
        }
    }

    Ok(())
}
