//! DHCP client

use std::net::Ipv4Addr;
use std::time::Duration;
use thiserror::Error;
use tracing::{info, debug, warn};

/// DHCP-specific errors with actionable context
#[derive(Error, Debug)]
pub enum DhcpError {
    #[error("failed to create DHCP socket: {0}")]
    SocketCreation(std::io::Error),

    #[error("interface not found: {interface}")]
    InterfaceNotFound { interface: String },

    #[error("DHCP {phase} timeout after {timeout_secs}s on {interface}")]
    Timeout {
        phase: &'static str,
        timeout_secs: u64,
        interface: String,
    },

    #[error("failed to send DHCP packet: {0}")]
    SendFailed(std::io::Error),

    #[error("invalid interface name: {0}")]
    InvalidInterfaceName(#[from] std::ffi::NulError),
}

type Result<T> = std::result::Result<T, DhcpError>;

/// DHCP lease
#[derive(Debug, Clone)]
pub struct DhcpLease {
    pub address: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub gateway: Option<Ipv4Addr>,
    pub dns_servers: Vec<Ipv4Addr>,
    pub lease_time: Duration,
    pub renewal_time: Duration,
    pub rebind_time: Duration,
    pub server: Ipv4Addr,
}

/// DHCP client
pub struct DhcpClient {
    interface: String,
    socket: i32,
}

impl DhcpClient {
    pub fn new(interface: &str) -> Result<Self> {
        // Create raw socket for DHCP
        let socket = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_DGRAM,
                (libc::ETH_P_IP as u16).to_be() as i32,
            )
        };

        if socket < 0 {
            return Err(DhcpError::SocketCreation(std::io::Error::last_os_error()));
        }

        Ok(Self {
            interface: interface.to_string(),
            socket,
        })
    }

    /// Request DHCP lease
    pub async fn request(&self) -> Result<DhcpLease> {
        info!("Requesting DHCP lease on {}", self.interface);

        // DHCP discovery
        let xid = rand::random::<u32>();
        self.send_discover(xid)?;

        // Wait for offer
        let offer = self.receive_offer(xid).await?;
        debug!("Received DHCP offer: {:?}", offer.address);

        // DHCP request
        self.send_request(xid, &offer)?;

        // Wait for ACK
        let lease = self.receive_ack(xid).await?;

        info!(
            "Obtained DHCP lease: {} (gateway: {:?})",
            lease.address,
            lease.gateway
        );

        Ok(lease)
    }

    fn send_discover(&self, xid: u32) -> Result<()> {
        let packet = self.build_discover_packet(xid);
        self.send_packet(&packet)?;
        Ok(())
    }

    fn build_discover_packet(&self, xid: u32) -> Vec<u8> {
        let mut packet = vec![0u8; 300];

        // BOOTP header
        packet[0] = 1;  // op: BOOTREQUEST
        packet[1] = 1;  // htype: Ethernet
        packet[2] = 6;  // hlen: MAC address length
        packet[3] = 0;  // hops

        // Transaction ID
        packet[4..8].copy_from_slice(&xid.to_be_bytes());

        // Flags: broadcast
        packet[10] = 0x80;

        // Magic cookie
        packet[236..240].copy_from_slice(&[99, 130, 83, 99]);

        // DHCP Message Type: Discover
        packet[240] = 53;  // Option type
        packet[241] = 1;   // Length
        packet[242] = 1;   // DHCPDISCOVER

        // Parameter Request List
        packet[243] = 55;  // Option type
        packet[244] = 4;   // Length
        packet[245] = 1;   // Subnet mask
        packet[246] = 3;   // Router
        packet[247] = 6;   // DNS
        packet[248] = 51;  // Lease time

        // End
        packet[249] = 255;

        packet
    }

    async fn receive_offer(&self, expected_xid: u32) -> Result<DhcpLease> {
        let timeout_secs = 10;
        let timeout = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if let Some(lease) = self.try_receive(expected_xid, 2)? {
                return Ok(lease);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(DhcpError::Timeout {
            phase: "offer",
            timeout_secs,
            interface: self.interface.clone(),
        })
    }

    fn send_request(&self, xid: u32, offer: &DhcpLease) -> Result<()> {
        let packet = self.build_request_packet(xid, offer);
        self.send_packet(&packet)?;
        Ok(())
    }

    fn build_request_packet(&self, xid: u32, offer: &DhcpLease) -> Vec<u8> {
        let mut packet = vec![0u8; 300];

        // BOOTP header
        packet[0] = 1;  // op: BOOTREQUEST
        packet[1] = 1;  // htype
        packet[2] = 6;  // hlen
        packet[3] = 0;  // hops

        // Transaction ID
        packet[4..8].copy_from_slice(&xid.to_be_bytes());

        // Flags: broadcast
        packet[10] = 0x80;

        // Magic cookie
        packet[236..240].copy_from_slice(&[99, 130, 83, 99]);

        // DHCP Message Type: Request
        packet[240] = 53;
        packet[241] = 1;
        packet[242] = 3;  // DHCPREQUEST

        // Requested IP
        packet[243] = 50;
        packet[244] = 4;
        packet[245..249].copy_from_slice(&offer.address.octets());

        // Server identifier
        packet[249] = 54;
        packet[250] = 4;
        packet[251..255].copy_from_slice(&offer.server.octets());

        // End
        packet[255] = 255;

        packet
    }

    async fn receive_ack(&self, expected_xid: u32) -> Result<DhcpLease> {
        let timeout_secs = 10;
        let timeout = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if let Some(lease) = self.try_receive(expected_xid, 5)? {
                return Ok(lease);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(DhcpError::Timeout {
            phase: "ACK",
            timeout_secs,
            interface: self.interface.clone(),
        })
    }

    fn try_receive(&self, expected_xid: u32, expected_type: u8) -> Result<Option<DhcpLease>> {
        let mut buffer = vec![0u8; 1500];

        let len = unsafe {
            libc::recv(
                self.socket,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                libc::MSG_DONTWAIT,
            )
        };

        if len <= 0 {
            return Ok(None);
        }

        // Skip IP and UDP headers (28 bytes for raw socket)
        let dhcp_start = 28;
        let dhcp = &buffer[dhcp_start..len as usize];

        // Check XID
        let xid = u32::from_be_bytes([dhcp[4], dhcp[5], dhcp[6], dhcp[7]]);
        if xid != expected_xid {
            return Ok(None);
        }

        // Parse options
        let options_start = 240;  // After magic cookie
        let mut i = options_start;
        let mut msg_type = 0u8;
        let mut subnet = Ipv4Addr::new(255, 255, 255, 0);
        let mut gateway = None;
        let mut dns = Vec::new();
        let mut lease_time = 86400u32;
        let mut server = Ipv4Addr::new(0, 0, 0, 0);

        while i < dhcp.len() && dhcp[i] != 255 {
            let opt_type = dhcp[i];
            if opt_type == 0 {
                i += 1;
                continue;
            }

            let opt_len = dhcp[i + 1] as usize;

            match opt_type {
                53 => msg_type = dhcp[i + 2],
                1 if opt_len == 4 => {
                    subnet = Ipv4Addr::new(
                        dhcp[i + 2], dhcp[i + 3], dhcp[i + 4], dhcp[i + 5]
                    );
                }
                3 if opt_len >= 4 => {
                    gateway = Some(Ipv4Addr::new(
                        dhcp[i + 2], dhcp[i + 3], dhcp[i + 4], dhcp[i + 5]
                    ));
                }
                6 => {
                    for j in (0..opt_len).step_by(4) {
                        dns.push(Ipv4Addr::new(
                            dhcp[i + 2 + j],
                            dhcp[i + 3 + j],
                            dhcp[i + 4 + j],
                            dhcp[i + 5 + j],
                        ));
                    }
                }
                51 if opt_len == 4 => {
                    lease_time = u32::from_be_bytes([
                        dhcp[i + 2], dhcp[i + 3], dhcp[i + 4], dhcp[i + 5]
                    ]);
                }
                54 if opt_len == 4 => {
                    server = Ipv4Addr::new(
                        dhcp[i + 2], dhcp[i + 3], dhcp[i + 4], dhcp[i + 5]
                    );
                }
                _ => {}
            }

            i += 2 + opt_len;
        }

        if msg_type != expected_type {
            return Ok(None);
        }

        // Get offered address from yiaddr
        let address = Ipv4Addr::new(dhcp[16], dhcp[17], dhcp[18], dhcp[19]);

        Ok(Some(DhcpLease {
            address,
            subnet_mask: subnet,
            gateway,
            dns_servers: dns,
            lease_time: Duration::from_secs(lease_time as u64),
            renewal_time: Duration::from_secs(lease_time as u64 / 2),
            rebind_time: Duration::from_secs(lease_time as u64 * 7 / 8),
            server,
        }))
    }

    fn send_packet(&self, packet: &[u8]) -> Result<()> {
        // Send to broadcast
        let dest = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: (libc::ETH_P_IP as u16).to_be(),
            sll_ifindex: self.get_ifindex()?,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 6,
            sll_addr: [0xff; 8],  // Broadcast
        };

        let result = unsafe {
            libc::sendto(
                self.socket,
                packet.as_ptr() as *const libc::c_void,
                packet.len(),
                0,
                &dest as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };

        if result < 0 {
            return Err(DhcpError::SendFailed(std::io::Error::last_os_error()));
        }

        Ok(())
    }

    fn get_ifindex(&self) -> Result<i32> {
        let name = std::ffi::CString::new(self.interface.as_str())?;
        let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if index == 0 {
            return Err(DhcpError::InterfaceNotFound {
                interface: self.interface.clone(),
            });
        }
        Ok(index as i32)
    }
}

impl Drop for DhcpClient {
    fn drop(&mut self) {
        unsafe { libc::close(self.socket) };
    }
}
