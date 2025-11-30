//! irisctl - Iris control utility

mod backlight;
mod config;
mod display;
mod ipc;

use crate::ipc::{IpcClient, IpcRequest};
use anyhow::Result;
use clap::{Parser, Subcommand};

/// Iris control utility
#[derive(Parser)]
#[command(name = "irisctl", version, about = "Control the Iris display daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/iris/iris.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// List connected displays
    List,

    /// Show display information
    Display {
        /// Display name
        name: String,
    },

    /// Set display mode
    Mode {
        /// Display name
        name: String,
        /// Resolution (e.g., 1920x1080)
        resolution: String,
        /// Refresh rate
        #[arg(short, long, default_value = "60")]
        refresh: f32,
    },

    /// Set display as primary
    Primary {
        /// Display name
        name: String,
    },

    /// Rotate display
    Rotate {
        /// Display name
        name: String,
        /// Rotation (0, 90, 180, 270)
        degrees: u16,
    },

    /// Backlight/brightness control
    Brightness {
        #[command(subcommand)]
        command: BrightnessCommands,
    },

    /// Night light control
    NightLight {
        #[command(subcommand)]
        command: NightLightCommands,
    },

    /// Show full daemon info
    Info,
}

#[derive(Subcommand)]
enum BrightnessCommands {
    /// Show current brightness
    Get,

    /// Set brightness percentage
    Set {
        /// Brightness percentage (0-100)
        percent: u8,
    },

    /// Increase brightness
    Up {
        /// Step size
        #[arg(default_value = "10")]
        step: u8,
    },

    /// Decrease brightness
    Down {
        /// Step size
        #[arg(default_value = "10")]
        step: u8,
    },
}

#[derive(Subcommand)]
enum NightLightCommands {
    /// Show night light status
    Status,

    /// Enable night light
    On,

    /// Disable night light
    Off,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = IpcClient::new(&cli.socket);

    match cli.command {
        Commands::List => {
            let displays = client.list_displays().await?;

            println!("Connected Displays");
            println!("==================");

            if displays.is_empty() {
                println!("No displays detected");
            } else {
                for display in &displays {
                    let primary = if display.primary { " (primary)" } else { "" };
                    let status = format!("{:?}", display.status).to_lowercase();
                    println!("{}{}: {} [{}]", display.name, primary, status, format!("{:?}", display.connection).to_lowercase());

                    if let Some(mode) = &display.current_mode {
                        println!("  Mode: {}x{}@{:.0}Hz", mode.width, mode.height, mode.refresh);
                    }
                    if display.rotation != 0 {
                        println!("  Rotation: {}°", display.rotation);
                    }
                }
            }
        }

        Commands::Display { name } => {
            match client.send(IpcRequest::GetDisplay { name: name.clone() }).await? {
                ipc::IpcResponse::Success { data } => {
                    let display: display::DisplayInfo = serde_json::from_value(data)?;

                    println!("Display: {}", display.name);
                    println!("=========={}", "=".repeat(display.name.len()));
                    println!("Connection: {:?}", display.connection);
                    println!("Status:     {:?}", display.status);
                    println!("Primary:    {}", if display.primary { "yes" } else { "no" });
                    println!("Enabled:    {}", if display.enabled { "yes" } else { "no" });
                    println!("Position:   ({}, {})", display.position.0, display.position.1);
                    println!("Rotation:   {}°", display.rotation);
                    println!("Scale:      {}x", display.scale);

                    if let Some(mode) = &display.current_mode {
                        println!("\nCurrent Mode: {}x{}@{:.2}Hz", mode.width, mode.height, mode.refresh);
                    }

                    if !display.modes.is_empty() {
                        println!("\nAvailable Modes:");
                        for mode in &display.modes {
                            let markers = format!(
                                "{}{}",
                                if mode.preferred { " *" } else { "" },
                                if mode.current { " (current)" } else { "" }
                            );
                            println!("  {}x{}@{:.0}Hz{}", mode.width, mode.height, mode.refresh, markers);
                        }
                    }
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Mode {
            name,
            resolution,
            refresh,
        } => {
            let parts: Vec<&str> = resolution.split('x').collect();
            if parts.len() != 2 {
                eprintln!("Invalid resolution format. Use WIDTHxHEIGHT (e.g., 1920x1080)");
                return Ok(());
            }

            let width: u32 = parts[0].parse()?;
            let height: u32 = parts[1].parse()?;

            match client
                .send(IpcRequest::SetMode {
                    name: name.clone(),
                    width,
                    height,
                    refresh,
                })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Set {} to {}x{}@{:.0}Hz", name, width, height, refresh);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Primary { name } => {
            match client
                .send(IpcRequest::SetPrimary { name: name.clone() })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Set {} as primary display", name);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Rotate { name, degrees } => {
            match client
                .send(IpcRequest::SetRotation {
                    name: name.clone(),
                    rotation: degrees,
                })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Rotated {} to {}°", name, degrees);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Brightness { command } => match command {
            BrightnessCommands::Get => {
                let info = client.get_backlight().await?;
                println!("Brightness: {}%", info.percent);
                println!("Device:     {} ({})", info.name, info.device_type);
            }

            BrightnessCommands::Set { percent } => {
                client.set_brightness(percent).await?;
                println!("Brightness set to {}%", percent);
            }

            BrightnessCommands::Up { step } => {
                match client.send(IpcRequest::IncreaseBrightness { step }).await? {
                    ipc::IpcResponse::Success { data } => {
                        let brightness = data["brightness"].as_u64().unwrap_or(0);
                        println!("Brightness: {}%", brightness);
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }

            BrightnessCommands::Down { step } => {
                match client.send(IpcRequest::DecreaseBrightness { step }).await? {
                    ipc::IpcResponse::Success { data } => {
                        let brightness = data["brightness"].as_u64().unwrap_or(0);
                        println!("Brightness: {}%", brightness);
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }
        },

        Commands::NightLight { command } => match command {
            NightLightCommands::Status => {
                match client.send(IpcRequest::GetNightLight).await? {
                    ipc::IpcResponse::Success { data } => {
                        let status: ipc::NightLightStatus = serde_json::from_value(data)?;
                        println!("Night Light");
                        println!("===========");
                        println!("Enabled:     {}", if status.enabled { "yes" } else { "no" });
                        println!("Active:      {}", if status.active { "yes" } else { "no" });
                        println!("Temperature: {}K", status.temperature);
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }

            NightLightCommands::On => {
                match client.send(IpcRequest::SetNightLight { enabled: true }).await? {
                    ipc::IpcResponse::Success { .. } => {
                        println!("Night light enabled");
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }

            NightLightCommands::Off => {
                match client.send(IpcRequest::SetNightLight { enabled: false }).await? {
                    ipc::IpcResponse::Success { .. } => {
                        println!("Night light disabled");
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }
        },

        Commands::Info => {
            let status = client.get_status().await?;

            println!("Iris Daemon Status");
            println!("==================");
            println!("Version: {}", status.version);
            println!();

            println!("Displays: {}", status.displays.len());
            for display in &status.displays {
                let primary = if display.primary { " (primary)" } else { "" };
                println!("  - {}{}: {:?}", display.name, primary, display.status);
            }
            println!();

            if let Some(backlight) = &status.backlight {
                println!("Backlight: {}% ({})", backlight.percent, backlight.name);
            } else {
                println!("Backlight: not available");
            }
            println!();

            println!("Night Light: {}", if status.night_light.enabled { "enabled" } else { "disabled" });
            println!("  Temperature: {}K", status.night_light.temperature);
        }
    }

    Ok(())
}
