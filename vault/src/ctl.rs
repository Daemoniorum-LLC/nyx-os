//! vaultctl - Vault control utility

mod config;
mod crypto;
mod ipc;
mod store;

use crate::ipc::{IpcClient, IpcRequest};
use crate::store::SecretType;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{self, Write};

/// Vault control utility
#[derive(Parser)]
#[command(name = "vaultctl", version, about = "Control the Vault secrets daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Socket path
    #[arg(long, default_value = "/run/vault/vault.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize new vault
    Init,

    /// Unlock vault
    Unlock,

    /// Lock vault
    Lock,

    /// Show vault status
    Status,

    /// Set a secret
    Set {
        /// Secret name
        name: String,
        /// Secret value (omit for interactive input)
        value: Option<String>,
        /// Secret type
        #[arg(short, long, default_value = "generic")]
        r#type: String,
    },

    /// Get a secret
    Get {
        /// Secret name
        name: String,
    },

    /// Delete a secret
    Delete {
        /// Secret name
        name: String,
    },

    /// List all secrets
    List,

    /// Search secrets by tag
    Search {
        /// Tag to search for
        tag: String,
    },

    /// Tag management
    Tag {
        #[command(subcommand)]
        command: TagCommands,
    },

    /// Generate a random password
    Generate {
        /// Password length
        #[arg(short, long, default_value = "20")]
        length: usize,
    },

    /// Create backup
    Backup,

    /// Change master password
    ChangePassword,
}

#[derive(Subcommand)]
enum TagCommands {
    /// Add tag to secret
    Add {
        /// Secret name
        name: String,
        /// Tag
        tag: String,
    },
}

fn read_password(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    // In a real implementation, this would disable echo
    // For now, just read the line
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;

    Ok(password.trim().to_string())
}

fn parse_secret_type(s: &str) -> SecretType {
    match s.to_lowercase().as_str() {
        "password" => SecretType::Password,
        "apikey" | "api_key" => SecretType::ApiKey,
        "sshkey" | "ssh_key" => SecretType::SshKey,
        "certificate" | "cert" => SecretType::Certificate,
        "token" => SecretType::Token,
        _ => SecretType::Generic,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = IpcClient::new(&cli.socket);

    match cli.command {
        Commands::Init => {
            match client.send(IpcRequest::Exists).await? {
                ipc::IpcResponse::Success { data } => {
                    if data["exists"].as_bool().unwrap_or(false) {
                        eprintln!("Vault already exists. Use 'unlock' to access it.");
                        return Ok(());
                    }
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                    return Ok(());
                }
            }

            let password = read_password("Enter master password: ")?;
            let confirm = read_password("Confirm master password: ")?;

            if password != confirm {
                eprintln!("Passwords do not match");
                return Ok(());
            }

            if password.len() < 8 {
                eprintln!("Password must be at least 8 characters");
                return Ok(());
            }

            match client.send(IpcRequest::Initialize { password }).await? {
                ipc::IpcResponse::Success { .. } => {
                    println!("Vault initialized and unlocked");
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Unlock => {
            let password = read_password("Master password: ")?;

            match client.send(IpcRequest::Unlock { password }).await? {
                ipc::IpcResponse::Success { .. } => {
                    println!("Vault unlocked");
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Lock => {
            match client.send(IpcRequest::Lock).await? {
                ipc::IpcResponse::Success { .. } => {
                    println!("Vault locked");
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Status => {
            let status = client.get_status().await?;

            println!("Vault Status");
            println!("============");
            println!("Version:  {}", status.version);
            println!("Exists:   {}", if status.vault_exists { "yes" } else { "no" });
            println!("Unlocked: {}", if status.unlocked { "yes" } else { "no" });

            if let Some(stats) = status.stats {
                println!();
                println!("Secrets:  {}", stats.total_secrets);
                if !stats.by_type.is_empty() {
                    println!("By type:");
                    for (secret_type, count) in &stats.by_type {
                        println!("  {:?}: {}", secret_type, count);
                    }
                }
            }
        }

        Commands::Set { name, value, r#type } => {
            let value = match value {
                Some(v) => v,
                None => read_password(&format!("Enter value for '{}': ", name))?,
            };

            let secret_type = parse_secret_type(&r#type);

            match client
                .send(IpcRequest::Set {
                    name: name.clone(),
                    value,
                    secret_type: Some(secret_type),
                })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Secret '{}' saved", name);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Get { name } => {
            match client
                .send(IpcRequest::Get { name: name.clone() })
                .await?
            {
                ipc::IpcResponse::Success { data } => {
                    let value = data["value"].as_str().unwrap_or("");
                    println!("{}", value);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Delete { name } => {
            print!("Delete secret '{}'? [y/N] ", name);
            io::stdout().flush()?;

            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;

            if confirm.trim().to_lowercase() != "y" {
                println!("Cancelled");
                return Ok(());
            }

            match client
                .send(IpcRequest::Delete { name: name.clone() })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Secret '{}' deleted", name);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::List => {
            let secrets = client.list().await?;

            println!("Secrets");
            println!("=======");

            if secrets.is_empty() {
                println!("No secrets stored");
            } else {
                for secret in &secrets {
                    let tags = if secret.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", secret.tags.join(", "))
                    };
                    println!(
                        "  {} ({:?}){}",
                        secret.name, secret.secret_type, tags
                    );
                }
                println!();
                println!("Total: {} secrets", secrets.len());
            }
        }

        Commands::Search { tag } => {
            match client.send(IpcRequest::SearchByTag { tag: tag.clone() }).await? {
                ipc::IpcResponse::Success { data } => {
                    let secrets: Vec<store::SecretMetadata> = serde_json::from_value(data)?;

                    println!("Secrets with tag '{}'", tag);
                    println!("{}", "=".repeat(20 + tag.len()));

                    if secrets.is_empty() {
                        println!("No secrets found");
                    } else {
                        for secret in &secrets {
                            println!("  {} ({:?})", secret.name, secret.secret_type);
                        }
                    }
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Tag { command } => match command {
            TagCommands::Add { name, tag } => {
                match client
                    .send(IpcRequest::AddTag {
                        name: name.clone(),
                        tag: tag.clone(),
                    })
                    .await?
                {
                    ipc::IpcResponse::Success { .. } => {
                        println!("Tag '{}' added to '{}'", tag, name);
                    }
                    ipc::IpcResponse::Error { message } => {
                        eprintln!("Error: {}", message);
                    }
                }
            }
        },

        Commands::Generate { length } => {
            match client
                .send(IpcRequest::GeneratePassword {
                    length: Some(length),
                })
                .await?
            {
                ipc::IpcResponse::Success { data } => {
                    let password = data["password"].as_str().unwrap_or("");
                    println!("{}", password);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::Backup => {
            match client.send(IpcRequest::Backup).await? {
                ipc::IpcResponse::Success { data } => {
                    let path = data["backup_path"].as_str().unwrap_or("unknown");
                    println!("Backup created: {}", path);
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }

        Commands::ChangePassword => {
            let old_password = read_password("Current password: ")?;
            let new_password = read_password("New password: ")?;
            let confirm = read_password("Confirm new password: ")?;

            if new_password != confirm {
                eprintln!("Passwords do not match");
                return Ok(());
            }

            if new_password.len() < 8 {
                eprintln!("Password must be at least 8 characters");
                return Ok(());
            }

            match client
                .send(IpcRequest::ChangePassword {
                    old_password,
                    new_password,
                })
                .await?
            {
                ipc::IpcResponse::Success { .. } => {
                    println!("Master password changed");
                }
                ipc::IpcResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                }
            }
        }
    }

    Ok(())
}
