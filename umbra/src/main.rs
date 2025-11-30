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
use std::io::{self, Write, BufRead};
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
    let mut shell = shell::Shell::new(config)?;

    // Create event channel
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    // Event handler task
    let handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                shell::ShellEvent::Output(msg) => println!("{}", msg),
                shell::ShellEvent::Error(msg) => eprintln!("{}", msg),
                shell::ShellEvent::Exit(code) => {
                    if code != 0 {
                        eprintln!("Exit code: {}", code);
                    }
                }
                shell::ShellEvent::JobStarted(id) => info!("Job {} started", id),
                shell::ShellEvent::JobFinished(id, code) => info!("Job {} finished ({})", id, code),
                shell::ShellEvent::DirectoryChanged(path) => {
                    info!("Changed directory to {:?}", path);
                }
            }
        }
    });

    // Execute based on mode
    if let Some(cmd) = args.command {
        // Single command mode
        shell.execute(&cmd, event_tx.clone()).await?;
    } else if let Some(script) = args.script {
        // Script mode
        let contents = std::fs::read_to_string(&script)?;
        for line in contents.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                if let Err(e) = shell.execute(line, event_tx.clone()).await {
                    // Check for exit command
                    let err_str = e.to_string();
                    if err_str.starts_with("exit:") {
                        break;
                    }
                    eprintln!("Error: {}", e);
                }
            }
        }
    } else {
        // Interactive mode
        info!("Starting Umbra interactive shell");

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            // Print prompt
            print!("{}$ ", shell.cwd().display());
            stdout.flush()?;

            // Read line
            let mut input = String::new();
            if stdin.lock().read_line(&mut input)? == 0 {
                break; // EOF
            }

            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            // Execute
            match shell.execute(input, event_tx.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.starts_with("exit:") {
                        let code: i32 = err_str.strip_prefix("exit:")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        std::process::exit(code);
                    }
                    eprintln!("Error: {}", e);
                }
            }

            // Check background jobs
            let completed = shell.check_jobs();
            for (id, code) in completed {
                println!("[{}] Done ({})", id, code);
            }
        }
    }

    drop(event_tx);
    let _ = handle.await;

    Ok(())
}
