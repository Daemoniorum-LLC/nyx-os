//! Health check implementation

use crate::service::{HealthCheck, HealthCheckType};
use anyhow::Result;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Result of a health check
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Whether the check passed
    pub healthy: bool,
    /// Optional message
    pub message: Option<String>,
    /// Duration the check took
    pub duration: Duration,
}

/// Run a health check
pub async fn run_health_check(config: &HealthCheck) -> HealthCheckResult {
    let start = std::time::Instant::now();

    let check_timeout = Duration::from_secs(config.timeout_sec as u64);

    let result = match &config.check_type {
        HealthCheckType::Http { url, expected_status } => {
            check_http(url, *expected_status, check_timeout).await
        }
        HealthCheckType::Tcp { host, port } => {
            check_tcp(host, *port, check_timeout).await
        }
        HealthCheckType::Socket { path } => {
            check_socket(path, check_timeout).await
        }
        HealthCheckType::Command { cmd, args } => {
            check_command(cmd, args, check_timeout).await
        }
        HealthCheckType::Ipc { endpoint } => {
            check_ipc(endpoint, check_timeout).await
        }
    };

    HealthCheckResult {
        healthy: result.is_ok(),
        message: result.err().map(|e| e.to_string()),
        duration: start.elapsed(),
    }
}

/// HTTP health check
async fn check_http(url: &str, expected_status: Option<u16>, timeout_dur: Duration) -> Result<()> {
    debug!("HTTP health check: {}", url);

    // In a real implementation, we'd use reqwest or similar
    // For now, just simulate
    let _ = (url, expected_status, timeout_dur);

    // TODO: Implement actual HTTP check
    Ok(())
}

/// TCP connection health check
async fn check_tcp(host: &str, port: u16, timeout_dur: Duration) -> Result<()> {
    debug!("TCP health check: {}:{}", host, port);

    let addr = format!("{}:{}", host, port);
    let result = timeout(timeout_dur, TcpStream::connect(&addr)).await;

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            warn!("TCP health check failed: {}", e);
            Err(e.into())
        }
        Err(_) => {
            warn!("TCP health check timed out");
            Err(anyhow::anyhow!("Connection timed out"))
        }
    }
}

/// Unix socket health check
async fn check_socket(path: &str, timeout_dur: Duration) -> Result<()> {
    debug!("Socket health check: {}", path);

    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let result = timeout(timeout_dur, UnixStream::connect(path)).await;

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                warn!("Socket health check failed: {}", e);
                Err(e.into())
            }
            Err(_) => {
                warn!("Socket health check timed out");
                Err(anyhow::anyhow!("Connection timed out"))
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (path, timeout_dur);
        Err(anyhow::anyhow!("Unix sockets not supported on this platform"))
    }
}

/// Command execution health check
async fn check_command(cmd: &str, args: &[String], timeout_dur: Duration) -> Result<()> {
    debug!("Command health check: {} {:?}", cmd, args);

    let result = timeout(
        timeout_dur,
        tokio::process::Command::new(cmd)
            .args(args)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(anyhow::anyhow!("Command failed: {}", stderr))
            }
        }
        Ok(Err(e)) => {
            warn!("Command health check failed: {}", e);
            Err(e.into())
        }
        Err(_) => {
            warn!("Command health check timed out");
            Err(anyhow::anyhow!("Command timed out"))
        }
    }
}

/// IPC health check (ping the service endpoint)
async fn check_ipc(endpoint: &str, timeout_dur: Duration) -> Result<()> {
    debug!("IPC health check: {}", endpoint);

    // TODO: Implement actual IPC ping when we have the IPC library
    let _ = (endpoint, timeout_dur);

    Ok(())
}
