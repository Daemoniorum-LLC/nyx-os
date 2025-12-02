//! cipher - Keyring CLI tool

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use nyx_cipher::ipc::{IpcRequest, IpcResponse};

#[derive(Parser)]
#[command(name = "cipher")]
#[command(about = "Nyx keyring management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/cipher/cipher.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show keyring status
    Status,

    /// Initialize keyring with master password
    Init,

    /// Unlock keyring
    Unlock,

    /// Lock keyring
    Lock,

    /// List collections
    Collections,

    /// Create a new collection
    CreateCollection {
        /// Collection name
        name: String,

        /// Collection label
        #[arg(long)]
        label: Option<String>,
    },

    /// List items in a collection
    List {
        /// Collection name
        #[arg(default_value = "default")]
        collection: String,
    },

    /// Store a secret
    Store {
        /// Item ID
        id: String,

        /// Item label
        #[arg(long)]
        label: Option<String>,

        /// Collection
        #[arg(long, default_value = "default")]
        collection: String,

        /// Attributes (key=value)
        #[arg(long, short)]
        attr: Vec<String>,
    },

    /// Get a secret
    Get {
        /// Item ID
        id: String,

        /// Collection
        #[arg(long, default_value = "default")]
        collection: String,
    },

    /// Delete a secret
    Delete {
        /// Item ID
        id: String,

        /// Collection
        #[arg(long, default_value = "default")]
        collection: String,
    },

    /// Search for secrets
    Search {
        /// Collection
        #[arg(long, default_value = "default")]
        collection: String,

        /// Attributes to match (key=value)
        #[arg(long, short)]
        attr: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let request = match cli.command {
        Commands::Status => IpcRequest::Status,

        Commands::Init => {
            let password = read_password("Enter master password: ")?;
            let confirm = read_password("Confirm password: ")?;

            if password != confirm {
                eprintln!("Passwords do not match");
                std::process::exit(1);
            }

            IpcRequest::Initialize { password }
        }

        Commands::Unlock => {
            let password = read_password("Enter password: ")?;
            IpcRequest::Unlock { password }
        }

        Commands::Lock => IpcRequest::Lock,

        Commands::Collections => IpcRequest::ListCollections,

        Commands::CreateCollection { name, label } => {
            IpcRequest::CreateCollection {
                name: name.clone(),
                label: label.unwrap_or(name),
            }
        }

        Commands::List { collection } => {
            IpcRequest::ListItems { collection }
        }

        Commands::Store { id, label, collection, attr } => {
            let secret = read_password("Enter secret: ")?;

            let attributes: HashMap<String, String> = attr.iter()
                .filter_map(|a| a.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            IpcRequest::StoreSecret {
                collection,
                id: id.clone(),
                label: label.unwrap_or(id),
                secret,
                attributes,
            }
        }

        Commands::Get { id, collection } => {
            // First open a session
            let session = open_session(&cli.socket).await?;

            IpcRequest::GetSecret {
                collection,
                id,
                session,
            }
        }

        Commands::Delete { id, collection } => {
            IpcRequest::DeleteSecret { collection, id }
        }

        Commands::Search { collection, attr } => {
            let attributes: HashMap<String, String> = attr.iter()
                .filter_map(|a| a.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            IpcRequest::Search { collection, attributes }
        }
    };

    let response = send_request(&cli.socket, request).await?;
    print_response(&response);

    Ok(())
}

fn read_password(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    // Disable echo
    let mut termios = termios::Termios::from_fd(0)?;
    let original = termios;
    termios.c_lflag &= !termios::ECHO;
    termios::tcsetattr(0, termios::TCSANOW, &termios)?;

    let mut password = String::new();
    io::stdin().read_line(&mut password)?;

    // Restore echo
    termios::tcsetattr(0, termios::TCSANOW, &original)?;
    println!();

    Ok(password.trim().to_string())
}

async fn open_session(socket_path: &str) -> Result<String> {
    let response = send_request(socket_path, IpcRequest::OpenSession).await?;

    match response {
        IpcResponse::Session { token } => Ok(token),
        IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        _ => Err(anyhow::anyhow!("Unexpected response")),
    }
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

fn print_response(response: &IpcResponse) {
    match response {
        IpcResponse::Success { message } => {
            println!("{}", message);
        }

        IpcResponse::Session { token } => {
            println!("Session: {}", token);
        }

        IpcResponse::Secret { value } => {
            println!("{}", value);
        }

        IpcResponse::Collections(collections) => {
            println!("{:<20} {:<30} {:<8}", "NAME", "LABEL", "LOCKED");
            for c in collections {
                let locked = if c.locked { "yes" } else { "no" };
                println!("{:<20} {:<30} {:<8}", c.name, c.label, locked);
            }
        }

        IpcResponse::Items(items) => {
            println!("{:<20} {:<40}", "ID", "LABEL");
            for item in items {
                println!("{:<20} {:<40}", item.id, item.label);
            }
        }

        IpcResponse::SearchResults(items) => {
            if items.is_empty() {
                println!("No matching items found");
            } else {
                println!("{:<20} {:<40}", "ID", "LABEL");
                for item in items {
                    println!("{:<20} {:<40}", item.id, item.label);
                }
            }
        }

        IpcResponse::Status { initialized, locked, collections, sessions } => {
            println!("Keyring Status:");
            println!("  Initialized: {}", if *initialized { "yes" } else { "no" });
            println!("  Locked:      {}", if *locked { "yes" } else { "no" });
            println!("  Collections: {}", collections);
            println!("  Sessions:    {}", sessions);
        }

        IpcResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
    }
}

// Simple termios bindings for password input
mod termios {
    pub const ECHO: libc::tcflag_t = libc::ECHO;
    pub const TCSANOW: i32 = libc::TCSANOW;

    #[derive(Clone, Copy)]
    pub struct Termios {
        inner: libc::termios,
        pub c_lflag: libc::tcflag_t,
    }

    impl Termios {
        pub fn from_fd(fd: i32) -> std::io::Result<Self> {
            let mut inner: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(fd, &mut inner) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self {
                c_lflag: inner.c_lflag,
                inner,
            })
        }
    }

    pub fn tcsetattr(fd: i32, action: i32, termios: &Termios) -> std::io::Result<()> {
        let mut inner = termios.inner;
        inner.c_lflag = termios.c_lflag;
        if unsafe { libc::tcsetattr(fd, action, &inner) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}
