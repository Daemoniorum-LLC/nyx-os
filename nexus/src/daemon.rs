//! Nexus daemon for privileged package operations

mod package;
mod repository;
mod resolver;
mod transaction;
mod store;
mod cache;
mod sandbox;
mod ipc;

use anyhow::Result;
use clap::Parser;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::ipc::{NexusServer, IpcRequest, IpcResponse};
use crate::repository::RepositoryManager;
use crate::store::PackageStore;
use crate::cache::PackageCache;

#[derive(Parser)]
#[command(name = "nexusd")]
#[command(about = "Nexus package manager daemon")]
struct Args {
    /// Socket path
    #[arg(long, default_value = "/run/nexus/nexus.sock")]
    socket: String,

    /// Store path
    #[arg(long, default_value = "/nyx/store")]
    store: String,

    /// Cache path
    #[arg(long, default_value = "/var/cache/nexus")]
    cache: String,

    /// Repository config directory
    #[arg(long, default_value = "/etc/nexus/repos.d")]
    repos: String,
}

struct DaemonState {
    store: PackageStore,
    repos: RepositoryManager,
    cache: PackageCache,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();

    info!("Starting Nexus daemon");

    // Initialize components
    let store = PackageStore::open(&args.store)?;
    let repos = RepositoryManager::load(&args.repos)?;
    let cache = PackageCache::open(&args.cache)?;

    let state = Arc::new(RwLock::new(DaemonState {
        store,
        repos,
        cache,
    }));

    // Start IPC server
    let server = NexusServer::new(&args.socket, state.clone());

    info!("Nexus daemon listening on {}", args.socket);

    // Handle requests
    server.run(|request, state| async move {
        handle_request(request, state).await
    }).await?;

    Ok(())
}

async fn handle_request(
    request: IpcRequest,
    state: Arc<RwLock<DaemonState>>,
) -> IpcResponse {
    match request {
        IpcRequest::Install { specs, dry_run } => {
            let read_guard = state.read().await;

            let resolver = resolver::DependencyResolver::new(&read_guard.store, &read_guard.repos);

            match resolver.resolve(&specs).await {
                Ok(plan) => {
                    if dry_run {
                        return IpcResponse::Plan {
                            install: plan.to_install.iter()
                                .map(|p| format!("{} {}", p.name, p.version))
                                .collect(),
                            remove: vec![],
                            download_size: plan.download_size,
                            install_size: plan.install_size,
                        };
                    }

                    drop(read_guard);
                    let write_guard = state.write().await;

                    let mut tx = transaction::Transaction::new(&write_guard.store);
                    for pkg in plan.to_install {
                        tx.add_install(pkg);
                    }

                    match tx.commit().await {
                        Ok(()) => IpcResponse::Success {
                            message: "Installation complete".to_string(),
                        },
                        Err(e) => IpcResponse::Error {
                            message: format!("Installation failed: {}", e),
                        },
                    }
                }
                Err(e) => IpcResponse::Error {
                    message: format!("Resolution failed: {}", e),
                },
            }
        }

        IpcRequest::Remove { packages, autoremove } => {
            let mut state = state.write().await;

            let mut tx = transaction::Transaction::new(&state.store);
            for name in &packages {
                tx.add_remove(name);
            }

            if autoremove {
                tx.add_autoremove();
            }

            match tx.commit().await {
                Ok(()) => IpcResponse::Success {
                    message: "Removal complete".to_string(),
                },
                Err(e) => IpcResponse::Error {
                    message: format!("Removal failed: {}", e),
                },
            }
        }

        IpcRequest::Upgrade { packages } => {
            let state_read = state.read().await;

            let upgrades = if packages.is_empty() {
                state_read.store.find_upgrades(&state_read.repos).await
            } else {
                state_read.store.find_upgrades_for(&state_read.repos, &packages).await
            };

            match upgrades {
                Ok(upgrades) => {
                    if upgrades.is_empty() {
                        return IpcResponse::Success {
                            message: "All packages are up to date".to_string(),
                        };
                    }

                    drop(state_read);
                    let mut state = state.write().await;

                    let mut tx = transaction::Transaction::new(&state.store);
                    for (_, new) in upgrades {
                        tx.add_install(new);
                    }

                    match tx.commit().await {
                        Ok(()) => IpcResponse::Success {
                            message: "Upgrade complete".to_string(),
                        },
                        Err(e) => IpcResponse::Error {
                            message: format!("Upgrade failed: {}", e),
                        },
                    }
                }
                Err(e) => IpcResponse::Error {
                    message: format!("Failed to check upgrades: {}", e),
                },
            }
        }

        IpcRequest::Sync => {
            let mut state = state.write().await;

            match state.repos.sync_all().await {
                Ok(()) => IpcResponse::Success {
                    message: "Repository sync complete".to_string(),
                },
                Err(e) => IpcResponse::Error {
                    message: format!("Sync failed: {}", e),
                },
            }
        }

        IpcRequest::Rollback { generation } => {
            let state = state.read().await;

            let gen = generation.unwrap_or_else(|| {
                state.store.current_generation().saturating_sub(1)
            });

            match state.store.activate_generation(gen) {
                Ok(()) => IpcResponse::Success {
                    message: format!("Rolled back to generation {}", gen),
                },
                Err(e) => IpcResponse::Error {
                    message: format!("Rollback failed: {}", e),
                },
            }
        }

        IpcRequest::Status => {
            let state = state.read().await;

            IpcResponse::Status {
                installed_count: state.store.list_installed()
                    .map(|p| p.len())
                    .unwrap_or(0),
                current_generation: state.store.current_generation(),
                cache_size: state.cache.size().unwrap_or(0),
            }
        }
    }
}
