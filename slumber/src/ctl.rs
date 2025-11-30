//! slumberctl - Slumber control utility

mod battery;
mod config;
mod ipc;
mod profiles;
mod sleep;

use crate::ipc::IpcClient;
use anyhow::Result;
use clap::{Parser, Subcommand};

/// Slumber control utility
#[derive(Parser)]
#[command(name = "slumberctl", version, about = "Control the Slumber power daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/slumber/slumber.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show power status (battery, AC)
    Status,

    /// Power profile management
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Battery information
    Battery,

    /// Sleep operations
    Sleep {
        #[command(subcommand)]
        command: SleepCommands,
    },

    /// Show full daemon info
    Info,
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Show current profile
    Show,

    /// Set power profile
    Set {
        /// Profile name (performance, balanced, powersave)
        name: String,
    },

    /// List available profiles
    List,
}

#[derive(Subcommand)]
enum SleepCommands {
    /// Show sleep status
    Status,

    /// Suspend to RAM
    Suspend,

    /// Hibernate to disk
    Hibernate,

    /// Hybrid sleep
    Hybrid,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = IpcClient::new(&cli.socket);

    match cli.command {
        Commands::Status => {
            let status = client.get_status().await?;

            println!("Power Status");
            println!("============");
            println!(
                "AC Power:      {}",
                if status.power.on_ac_power { "yes" } else { "no" }
            );
            println!("Battery:       {}%", status.power.total_capacity);
            println!(
                "State:         {:?}",
                status.power.combined_state
            );
            println!();
            println!("Profile:       {}", status.profile.current);
        }

        Commands::Profile { command } => match command {
            ProfileCommands::Show => {
                let profile = client.get_profile().await?;
                println!("Current Profile: {}", profile.current);
            }

            ProfileCommands::Set { name } => {
                client.set_profile(&name).await?;
                println!("Profile set to: {}", name);
            }

            ProfileCommands::List => {
                let profile = client.get_profile().await?;
                println!("Available Profiles");
                println!("==================");
                for p in &profile.available {
                    let marker = if p == &profile.current { " *" } else { "" };
                    println!("  {}{}", p, marker);
                }
            }
        },

        Commands::Battery => {
            let status = client.get_power_status().await?;

            println!("Battery Information");
            println!("===================");

            if status.batteries.is_empty() {
                println!("No batteries found");
            } else {
                for bat in &status.batteries {
                    println!("{}:", bat.name);
                    println!("  State:        {:?}", bat.state);
                    println!("  Capacity:     {}%", bat.capacity);

                    if let Some(health) = bat.health {
                        println!("  Health:       {}%", health);
                    }
                    if let Some(cycles) = bat.cycle_count {
                        println!("  Cycles:       {}", cycles);
                    }
                    if let Some(tech) = &bat.technology {
                        println!("  Technology:   {}", tech);
                    }
                    if let Some(time) = bat.time_to_empty {
                        let hours = time / 3600;
                        let mins = (time % 3600) / 60;
                        println!("  Time to empty: {}h {}m", hours, mins);
                    }
                    if let Some(time) = bat.time_to_full {
                        let hours = time / 3600;
                        let mins = (time % 3600) / 60;
                        println!("  Time to full:  {}h {}m", hours, mins);
                    }
                }
            }

            println!();
            println!("AC Adapters:");
            if status.ac_adapters.is_empty() {
                println!("  None found");
            } else {
                for ac in &status.ac_adapters {
                    let state = if ac.online { "connected" } else { "disconnected" };
                    println!("  {}: {}", ac.name, state);
                }
            }
        }

        Commands::Sleep { command } => match command {
            SleepCommands::Status => {
                let status = client.get_status().await?;

                println!("Sleep Status");
                println!("============");
                println!(
                    "Suspend:       {}",
                    if status.sleep.suspend_enabled { "enabled" } else { "disabled" }
                );
                println!(
                    "Hibernate:     {}",
                    if status.sleep.hibernate_enabled { "enabled" } else { "disabled" }
                );
                println!(
                    "Hybrid Sleep:  {}",
                    if status.sleep.hybrid_sleep_enabled { "enabled" } else { "disabled" }
                );
                println!("Method:        {}", status.sleep.suspend_method);
                println!();
                println!("Available States:");
                let states = &status.sleep.available_states;
                if states.suspend {
                    println!("  - suspend (mem)");
                }
                if states.hibernate {
                    println!("  - hibernate (disk)");
                }
                if states.freeze {
                    println!("  - freeze (s2idle)");
                }
                if states.standby {
                    println!("  - standby");
                }
            }

            SleepCommands::Suspend => {
                println!("Suspending...");
                client.suspend().await?;
                println!("Resumed from suspend");
            }

            SleepCommands::Hibernate => {
                println!("Hibernating...");
                client.hibernate().await?;
                println!("Resumed from hibernate");
            }

            SleepCommands::Hybrid => {
                println!("Hybrid sleep...");
                match client.send(ipc::IpcRequest::HybridSleep).await? {
                    ipc::IpcResponse::Success { .. } => println!("Resumed from hybrid sleep"),
                    ipc::IpcResponse::Error { message } => eprintln!("Error: {}", message),
                }
            }
        },

        Commands::Info => {
            let status = client.get_status().await?;

            println!("Slumber Daemon Status");
            println!("=====================");
            println!("Version:       {}", status.version);
            println!();

            println!("Power:");
            println!(
                "  AC:          {}",
                if status.power.on_ac_power { "connected" } else { "disconnected" }
            );
            println!("  Battery:     {}%", status.power.total_capacity);
            println!("  State:       {:?}", status.power.combined_state);
            println!();

            println!("Profile:");
            println!("  Current:     {}", status.profile.current);
            println!("  Available:   {}", status.profile.available.join(", "));
            println!();

            println!("Sleep:");
            println!(
                "  Suspend:     {}",
                if status.sleep.suspend_enabled { "enabled" } else { "disabled" }
            );
            println!(
                "  Hibernate:   {}",
                if status.sleep.hibernate_enabled { "enabled" } else { "disabled" }
            );
        }
    }

    Ok(())
}
