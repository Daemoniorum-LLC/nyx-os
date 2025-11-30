//! Socket activation support

use crate::lifecycle::LifecycleManager;
use crate::unit::{SocketConfig, SocketType, UnitRegistry};
use anyhow::{Result, Context};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Socket activator manages listening sockets and activates services on demand
pub struct SocketActivator {
    lifecycle: Arc<LifecycleManager>,
    runtime_dir: PathBuf,
    sockets: RwLock<HashMap<String, ActivatedSocket>>,
}

/// An activated socket that triggers service start
struct ActivatedSocket {
    service_name: String,
    config: SocketConfig,
    listener_fd: Option<RawFd>,
}

impl SocketActivator {
    pub fn new(lifecycle: Arc<LifecycleManager>, runtime_dir: PathBuf) -> Self {
        Self {
            lifecycle,
            runtime_dir,
            sockets: RwLock::new(HashMap::new()),
        }
    }

    /// Set up listening sockets for all socket-activated services
    pub async fn setup_sockets(&self, registry: &UnitRegistry) -> Result<()> {
        for unit in registry.all() {
            if let Some(socket_config) = &unit.socket {
                match self.setup_socket(&unit.name, socket_config).await {
                    Ok(()) => {
                        info!("Socket activation enabled for {}", unit.name);
                    }
                    Err(e) => {
                        error!("Failed to set up socket for {}: {}", unit.name, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Set up a single socket
    async fn setup_socket(&self, service_name: &str, config: &SocketConfig) -> Result<()> {
        let listen_addr = &config.listen;

        // Determine socket type from address
        if listen_addr.starts_with('/') || listen_addr.starts_with("unix:") {
            // Unix socket
            self.setup_unix_socket(service_name, config).await
        } else if listen_addr.contains(':') {
            // TCP socket
            self.setup_tcp_socket(service_name, config).await
        } else {
            Err(anyhow::anyhow!("Unknown socket address format: {}", listen_addr))
        }
    }

    /// Set up a Unix domain socket
    async fn setup_unix_socket(&self, service_name: &str, config: &SocketConfig) -> Result<()> {
        let path = if config.listen.starts_with("unix:") {
            PathBuf::from(&config.listen[5..])
        } else {
            PathBuf::from(&config.listen)
        };

        // Remove existing socket
        let _ = std::fs::remove_file(&path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create listener
        let listener = StdUnixListener::bind(&path)
            .with_context(|| format!("Failed to bind Unix socket: {:?}", path))?;

        // Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(config.mode))?;
        }

        let fd = listener.as_raw_fd();

        // Store socket info
        self.sockets.write().await.insert(service_name.to_string(), ActivatedSocket {
            service_name: service_name.to_string(),
            config: config.clone(),
            listener_fd: Some(fd),
        });

        // Spawn activation listener
        let lifecycle = self.lifecycle.clone();
        let service_name = service_name.to_string();
        let accept = config.accept;

        tokio::spawn(async move {
            let listener = unsafe {
                UnixListener::from_std(std::os::unix::net::UnixListener::from_raw_fd(fd))
            };

            if let Err(e) = listener {
                error!("Failed to convert Unix listener: {}", e);
                return;
            }

            let listener = listener.unwrap();

            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        debug!("Socket activation triggered for {}", service_name);

                        // Start the service if not running
                        if let Err(e) = lifecycle.start(&service_name).await {
                            error!("Socket activation failed for {}: {}", service_name, e);
                        }

                        if accept {
                            // For accept mode, we'd pass the connection to the service
                            // This is simplified - real implementation would use fd passing
                            drop(stream);
                        } else {
                            // For non-accept mode, service takes over the socket
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Socket accept error for {}: {}", service_name, e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Set up a TCP socket
    async fn setup_tcp_socket(&self, service_name: &str, config: &SocketConfig) -> Result<()> {
        let listener = TcpListener::bind(&config.listen).await
            .with_context(|| format!("Failed to bind TCP socket: {}", config.listen))?;

        let local_addr = listener.local_addr()?;
        info!("TCP socket listening on {} for {}", local_addr, service_name);

        // Store socket info
        self.sockets.write().await.insert(service_name.to_string(), ActivatedSocket {
            service_name: service_name.to_string(),
            config: config.clone(),
            listener_fd: None,
        });

        // Spawn activation listener
        let lifecycle = self.lifecycle.clone();
        let service_name = service_name.to_string();
        let accept = config.accept;
        let max_connections = config.max_connections;

        tokio::spawn(async move {
            let mut connection_count = 0u32;

            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        debug!("TCP connection from {} triggered activation for {}", addr, service_name);

                        if max_connections > 0 && connection_count >= max_connections {
                            warn!("Max connections reached for {}", service_name);
                            drop(stream);
                            continue;
                        }

                        connection_count += 1;

                        // Start the service if not running
                        if let Err(e) = lifecycle.start(&service_name).await {
                            error!("Socket activation failed for {}: {}", service_name, e);
                        }

                        if accept {
                            // Connection per instance mode
                            drop(stream);
                        } else {
                            // Service takes over
                            break;
                        }
                    }
                    Err(e) => {
                        error!("TCP accept error for {}: {}", service_name, e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Get file descriptors to pass to a service
    pub async fn get_fds(&self, service_name: &str) -> Vec<RawFd> {
        let sockets = self.sockets.read().await;

        sockets.get(service_name)
            .and_then(|s| s.listener_fd)
            .map(|fd| vec![fd])
            .unwrap_or_default()
    }

    /// Check if a service has socket activation
    pub async fn has_socket(&self, service_name: &str) -> bool {
        self.sockets.read().await.contains_key(service_name)
    }

    /// Remove socket for a service
    pub async fn remove_socket(&self, service_name: &str) {
        self.sockets.write().await.remove(service_name);
    }
}

/// Environment variables for socket activation (systemd compatible)
pub fn socket_activation_env(fds: &[RawFd], names: &[String]) -> Vec<(String, String)> {
    let mut env = Vec::new();

    // LISTEN_FDS - number of file descriptors
    env.push(("LISTEN_FDS".to_string(), fds.len().to_string()));

    // LISTEN_PID - process ID that should receive the sockets
    env.push(("LISTEN_PID".to_string(), std::process::id().to_string()));

    // LISTEN_FDNAMES - colon-separated names (optional)
    if !names.is_empty() {
        env.push(("LISTEN_FDNAMES".to_string(), names.join(":")));
    }

    env
}

/// Parse LISTEN_FDS environment for services
pub fn parse_listen_fds() -> Option<Vec<RawFd>> {
    let count: usize = std::env::var("LISTEN_FDS").ok()?.parse().ok()?;
    let pid: u32 = std::env::var("LISTEN_PID").ok()?.parse().ok()?;

    // Only accept if we're the intended recipient
    if pid != std::process::id() {
        return None;
    }

    // File descriptors start at 3 (after stdin, stdout, stderr)
    let fds: Vec<RawFd> = (3..(3 + count as RawFd)).collect();

    Some(fds)
}

use std::os::unix::io::FromRawFd;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_activation_env() {
        let fds = vec![3, 4];
        let names = vec!["http".to_string(), "https".to_string()];

        let env = socket_activation_env(&fds, &names);

        assert!(env.iter().any(|(k, v)| k == "LISTEN_FDS" && v == "2"));
        assert!(env.iter().any(|(k, v)| k == "LISTEN_FDNAMES" && v == "http:https"));
    }
}
