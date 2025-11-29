//! # Aether
//!
//! Display server and Wayland compositor for DaemonOS.
//!
//! ## Philosophy
//!
//! Aether is a security-first compositor that integrates with Guardian for
//! capability-based window and input management. Every window operation
//! goes through capability checks.
//!
//! ## Features
//!
//! - **Wayland Native**: Full Wayland protocol support
//! - **XWayland**: X11 application compatibility
//! - **Security Integration**: Guardian-mediated window permissions
//! - **GPU Acceleration**: Hardware-accelerated rendering
//! - **Multi-Output**: Multiple monitor support
//! - **HDR Ready**: High dynamic range display support
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         AETHER                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
//! │  │   Wayland   │  │   Input     │  │   Output    │         │
//! │  │   Server    │  │   Handler   │  │   Manager   │         │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
//! │         │                │                │                 │
//! │  ┌──────┴────────────────┴────────────────┴──────┐         │
//! │  │              Compositor Core                   │         │
//! │  └──────────────────────┬────────────────────────┘         │
//! │                         │                                   │
//! │  ┌──────────────────────┴────────────────────────┐         │
//! │  │              Renderer (OpenGL/Vulkan)          │         │
//! │  └───────────────────────────────────────────────┘         │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!           ┌──────────────────┼──────────────────┐
//!           ▼                  ▼                  ▼
//!    ┌───────────┐      ┌───────────┐      ┌───────────┐
//!    │  Guardian │      │   DRM/KMS │      │  XWayland │
//!    │  (Security)│      │  (Display)│      │  (X11)    │
//!    └───────────┘      └───────────┘      └───────────┘
//! ```

mod config;
mod compositor;
mod input;
mod output;
mod shell;
mod window;
mod render;
mod security;
mod ipc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, error};

/// Aether - Display server for DaemonOS
#[derive(Parser, Debug)]
#[command(name = "aether", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long, default_value = "/grimoire/system/aether.yaml")]
    config: PathBuf,

    /// Run in windowed mode (for development)
    #[arg(long)]
    windowed: bool,

    /// Socket path for Wayland clients
    #[arg(long)]
    socket: Option<String>,

    /// Enable XWayland
    #[arg(long, default_value = "true")]
    xwayland: bool,

    /// Guardian socket path
    #[arg(long, default_value = "/run/guardian/guardian.sock")]
    guardian_socket: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    info!("Aether v{} starting", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = config::load_config(&args.config)?;

    // Create compositor
    let mut compositor = compositor::Compositor::new(
        config,
        args.windowed,
        args.socket,
        args.xwayland,
        args.guardian_socket,
    )?;

    info!("Aether ready");

    // Run main loop
    compositor.run()
}
