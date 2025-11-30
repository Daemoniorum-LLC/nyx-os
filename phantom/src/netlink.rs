//! Netlink monitoring for device events

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use tracing::{debug, error};

/// Netlink device event
#[derive(Debug, Clone)]
pub struct NetlinkEvent {
    /// Action (add, remove, change, move, online, offline)
    pub action: String,
    /// Device path
    pub devpath: String,
    /// Subsystem
    pub subsystem: Option<String>,
    /// Device type
    pub devtype: Option<String>,
    /// Device node
    pub devname: Option<String>,
    /// Major number
    pub major: Option<u32>,
    /// Minor number
    pub minor: Option<u32>,
    /// Sequence number
    pub seqnum: Option<u64>,
    /// Additional properties
    pub properties: HashMap<String, String>,
}

/// Netlink socket monitor
pub struct NetlinkMonitor {
    socket: i32,
    buffer: Vec<u8>,
}

impl NetlinkMonitor {
    pub fn new() -> Result<Self> {
        // Create netlink socket
        let socket = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                15, // NETLINK_KOBJECT_UEVENT
            )
        };

        if socket < 0 {
            return Err(anyhow!("Failed to create netlink socket"));
        }

        // Bind to kernel uevents
        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = 1; // UDEV_MONITOR_KERNEL

        let result = unsafe {
            libc::bind(
                socket,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };

        if result < 0 {
            unsafe { libc::close(socket) };
            return Err(anyhow!("Failed to bind netlink socket"));
        }

        Ok(Self {
            socket,
            buffer: vec![0u8; 8192],
        })
    }

    /// Receive next device event
    pub async fn receive_event(&mut self) -> Result<Option<NetlinkEvent>> {
        // Wait for socket to be readable
        let fd = self.socket;

        // Use tokio to wait for readability
        let ready = tokio::task::spawn_blocking(move || {
            let mut fds = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let result = unsafe {
                libc::poll(&mut fds, 1, 100) // 100ms timeout
            };

            result > 0 && (fds.revents & libc::POLLIN) != 0
        }).await?;

        if !ready {
            return Ok(None);
        }

        // Read from socket
        let len = unsafe {
            libc::recv(
                self.socket,
                self.buffer.as_mut_ptr() as *mut libc::c_void,
                self.buffer.len(),
                0,
            )
        };

        if len <= 0 {
            return Ok(None);
        }

        // Parse uevent message
        let data = &self.buffer[..len as usize];
        self.parse_uevent(data)
    }

    fn parse_uevent(&self, data: &[u8]) -> Result<Option<NetlinkEvent>> {
        // Uevent format: "ACTION@DEVPATH\0KEY=VALUE\0..."
        let mut parts = data.split(|&b| b == 0);

        // First part is header: ACTION@DEVPATH
        let header = parts.next()
            .and_then(|h| std::str::from_utf8(h).ok())
            .ok_or_else(|| anyhow!("Invalid uevent header"))?;

        let (action, devpath) = header.split_once('@')
            .ok_or_else(|| anyhow!("Invalid uevent format"))?;

        let mut event = NetlinkEvent {
            action: action.to_string(),
            devpath: format!("/sys{}", devpath),
            subsystem: None,
            devtype: None,
            devname: None,
            major: None,
            minor: None,
            seqnum: None,
            properties: HashMap::new(),
        };

        // Parse key=value pairs
        for part in parts {
            if part.is_empty() {
                continue;
            }

            if let Ok(s) = std::str::from_utf8(part) {
                if let Some((key, value)) = s.split_once('=') {
                    match key {
                        "SUBSYSTEM" => event.subsystem = Some(value.to_string()),
                        "DEVTYPE" => event.devtype = Some(value.to_string()),
                        "DEVNAME" => event.devname = Some(value.to_string()),
                        "MAJOR" => event.major = value.parse().ok(),
                        "MINOR" => event.minor = value.parse().ok(),
                        "SEQNUM" => event.seqnum = value.parse().ok(),
                        _ => {
                            event.properties.insert(key.to_string(), value.to_string());
                        }
                    }
                }
            }
        }

        debug!(
            "Parsed uevent: {} {} {:?}",
            event.action, event.devpath, event.subsystem
        );

        Ok(Some(event))
    }
}

impl Drop for NetlinkMonitor {
    fn drop(&mut self) {
        unsafe { libc::close(self.socket) };
    }
}

/// Synthetic trigger for testing
pub fn trigger_event(action: &str, devpath: &str) -> Result<()> {
    // Write to /sys/.../uevent file
    let uevent_path = format!("{}/uevent", devpath);

    std::fs::write(&uevent_path, action)?;

    Ok(())
}

/// Trigger events for all devices in a subsystem
pub fn trigger_subsystem(subsystem: &str, action: &str) -> Result<()> {
    let class_path = format!("/sys/class/{}", subsystem);

    if !std::path::Path::new(&class_path).exists() {
        return Err(anyhow!("Subsystem not found: {}", subsystem));
    }

    for entry in std::fs::read_dir(&class_path)? {
        let entry = entry?;
        let uevent_path = entry.path().join("uevent");

        if uevent_path.exists() {
            let _ = std::fs::write(&uevent_path, action);
        }
    }

    Ok(())
}
