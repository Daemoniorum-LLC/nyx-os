//! scribectl - Journal query and control tool

mod journal;
mod collector;
mod storage;
mod query;
mod ipc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::ipc::{IpcRequest, IpcResponse};
use crate::query::OutputFormat;

#[derive(Parser)]
#[command(name = "scribectl")]
#[command(about = "Nyx journal query and control")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/scribe/scribe.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Query journal entries (default)
    Query {
        /// Show entries since time
        #[arg(long, short = 'S')]
        since: Option<String>,

        /// Show entries until time
        #[arg(long, short = 'U')]
        until: Option<String>,

        /// Filter by priority (emerg, alert, crit, err, warning, notice, info, debug)
        #[arg(long, short)]
        priority: Option<String>,

        /// Filter by identifier (unit name)
        #[arg(long, short = 'u')]
        unit: Option<String>,

        /// Grep pattern
        #[arg(long, short)]
        grep: Option<String>,

        /// Number of entries to show
        #[arg(long, short, default_value = "100")]
        lines: usize,

        /// Show newest entries first
        #[arg(long, short)]
        reverse: bool,

        /// Output format (short, verbose, json, cat)
        #[arg(long, short, default_value = "short")]
        output: String,

        /// Follow journal (like tail -f)
        #[arg(long, short)]
        follow: bool,
    },

    /// Show disk usage
    DiskUsage,

    /// Rotate journal
    Rotate,

    /// Remove old journal files
    Vacuum,

    /// Verify journal integrity
    Verify,

    /// Flush journal to disk
    Flush,

    /// Show kernel messages
    Dmesg {
        /// Number of lines
        #[arg(long, short, default_value = "100")]
        lines: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Query {
            since,
            until,
            priority,
            unit,
            grep,
            lines,
            reverse,
            output,
            follow,
        } => {
            let format = match output.as_str() {
                "verbose" => OutputFormat::Verbose,
                "json" => OutputFormat::Json,
                "cat" => OutputFormat::Cat,
                _ => OutputFormat::Short,
            };

            let request = IpcRequest::Query {
                since,
                until,
                priority: priority.and_then(|p| query::parse_priority(&p)).map(|p| p as u8),
                identifier: unit,
                grep,
                limit: Some(lines),
                reverse,
            };

            let response = send_request(&cli.socket, request).await?;

            match response {
                IpcResponse::Entries(entries) => {
                    for entry in entries {
                        let line = match format {
                            OutputFormat::Short => format!(
                                "{} {}[{}]: {}",
                                entry.timestamp,
                                entry.identifier,
                                entry.pid.map(|p| p.to_string()).unwrap_or_default(),
                                entry.message
                            ),
                            OutputFormat::Verbose => format!(
                                "{} [{}] {}.{} {}[{}]: {}",
                                entry.timestamp,
                                entry.priority,
                                entry.facility,
                                entry.priority,
                                entry.identifier,
                                entry.pid.map(|p| p.to_string()).unwrap_or_default(),
                                entry.message
                            ),
                            OutputFormat::Json => serde_json::to_string(&entry)?,
                            OutputFormat::Cat => entry.message,
                        };
                        println!("{}", line);
                    }
                }
                IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
                _ => {}
            }

            if follow {
                println!("-- Follow mode not yet implemented --");
            }
        }

        Commands::DiskUsage => {
            let response = send_request(&cli.socket, IpcRequest::DiskUsage).await?;

            match response {
                IpcResponse::DiskUsage {
                    total_size,
                    current_size,
                    compressed_size,
                    file_count,
                } => {
                    println!("Journal Disk Usage:");
                    println!("  Total:      {}", storage::DiskUsage::format_size(total_size));
                    println!("  Current:    {}", storage::DiskUsage::format_size(current_size));
                    println!("  Compressed: {}", storage::DiskUsage::format_size(compressed_size));
                    println!("  Files:      {}", file_count);
                }
                IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
                _ => {}
            }
        }

        Commands::Rotate => {
            let response = send_request(&cli.socket, IpcRequest::Rotate).await?;
            print_simple_response(&response);
        }

        Commands::Vacuum => {
            let response = send_request(&cli.socket, IpcRequest::Vacuum).await?;
            print_simple_response(&response);
        }

        Commands::Verify => {
            let response = send_request(&cli.socket, IpcRequest::Verify).await?;

            match response {
                IpcResponse::VerifyResult {
                    valid_entries,
                    valid_archives,
                    corrupted_files,
                } => {
                    println!("Journal Verification:");
                    println!("  Valid entries:  {}", valid_entries);
                    println!("  Valid archives: {}", valid_archives);
                    println!("  Corrupted files: {}", corrupted_files);

                    if corrupted_files > 0 {
                        println!("\nWARNING: Corrupted files detected!");
                    } else {
                        println!("\nJournal integrity: OK");
                    }
                }
                IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
                _ => {}
            }
        }

        Commands::Flush => {
            let response = send_request(&cli.socket, IpcRequest::Flush).await?;
            print_simple_response(&response);
        }

        Commands::Dmesg { lines } => {
            let request = IpcRequest::Query {
                since: None,
                until: None,
                priority: None,
                identifier: Some("kernel".to_string()),
                grep: None,
                limit: Some(lines),
                reverse: false,
            };

            let response = send_request(&cli.socket, request).await?;

            match response {
                IpcResponse::Entries(entries) => {
                    for entry in entries {
                        println!("[{}] {}", entry.priority, entry.message);
                    }
                }
                IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

async fn send_request(socket_path: &str, request: IpcRequest) -> Result<IpcResponse> {
    let mut stream = UnixStream::connect(socket_path).await?;

    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    Ok(serde_json::from_str(&line)?)
}

fn print_simple_response(response: &IpcResponse) {
    match response {
        IpcResponse::Success { message } => println!("{}", message),
        IpcResponse::Error { message } => eprintln!("Error: {}", message),
        _ => {}
    }
}
