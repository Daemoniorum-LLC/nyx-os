//! # Umbra
//!
//! Conversational shell for DaemonOS.
//!
//! ## Philosophy
//!
//! Umbra combines traditional shell functionality with AI-powered conversation.
//! Users can type commands OR ask questions naturally - Umbra figures out what to do.
//!
//! ## Features
//!
//! - **Hybrid Interface**: Traditional commands + natural language
//! - **Persona Integration**: Load personas for different interaction styles
//! - **Smart Completion**: AI-powered command suggestions
//! - **Context Awareness**: Remembers conversation history
//! - **Rich Output**: Syntax highlighting, tables, progress bars

mod config;
mod shell;
mod command;
mod completion;
mod history;
mod prompt;
mod ui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;

/// Umbra - Conversational shell
#[derive(Parser, Debug)]
#[command(name = "umbra", version, about)]
struct Args {
    /// Configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Execute command and exit
    #[arg(short = 'c', long)]
    command: Option<String>,

    /// Script file to execute
    script: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Disable AI features
    #[arg(long)]
    no_ai: bool,

    /// Persona to use
    #[arg(short, long)]
    persona: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();

    // Load configuration
    let config = config::load_config(args.config.as_deref())?;

    // Create shell
    let mut shell = shell::Shell::new(config, !args.no_ai, args.persona)?;

    // Execute based on mode
    if let Some(cmd) = args.command {
        // Single command mode
        shell.execute_line(&cmd).await?;
    } else if let Some(script) = args.script {
        // Script mode
        shell.execute_script(&script).await?;
    } else {
        // Interactive mode
        info!("Starting Umbra interactive shell");
        shell.run_interactive().await?;
    }

    Ok(())
}
