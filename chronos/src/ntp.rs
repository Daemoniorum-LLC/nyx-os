//! NTP client implementation
//!
//! Implements NTPv4 (RFC 5905) for network time synchronization.

use crate::config::NtpConfig;
use anyhow::{anyhow, Result};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// NTP port
const NTP_PORT: u16 = 123;

/// NTP packet size
const NTP_PACKET_SIZE: usize = 48;

/// Seconds between 1900 (NTP epoch) and 1970 (Unix epoch)
const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

/// NTP timestamp structure
#[derive(Debug, Clone, Copy, Default)]
pub struct NtpTimestamp {
    /// Seconds since NTP epoch (1900-01-01)
    pub seconds: u32,
    /// Fractional seconds (2^32 fractions of a second)
    pub fraction: u32,
}

impl NtpTimestamp {
    /// Create timestamp from current system time
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let seconds = (now.as_secs() + NTP_UNIX_OFFSET) as u32;
        let fraction = ((now.subsec_nanos() as u64 * (1u64 << 32)) / 1_000_000_000) as u32;

        Self { seconds, fraction }
    }

    /// Convert to Unix timestamp (seconds since 1970)
    pub fn to_unix_secs(&self) -> f64 {
        let secs = self.seconds as u64 - NTP_UNIX_OFFSET;
        let frac = self.fraction as f64 / (1u64 << 32) as f64;
        secs as f64 + frac
    }

    /// Create from byte slice (big-endian)
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let seconds = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let fraction = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        Self { seconds, fraction }
    }

    /// Convert to byte array (big-endian)
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        bytes
    }
}

/// NTP packet structure
#[derive(Debug, Clone)]
pub struct NtpPacket {
    /// Leap indicator (2 bits)
    pub leap: u8,
    /// Version number (3 bits)
    pub version: u8,
    /// Mode (3 bits)
    pub mode: u8,
    /// Stratum
    pub stratum: u8,
    /// Poll interval (log2 seconds)
    pub poll: i8,
    /// Precision (log2 seconds)
    pub precision: i8,
    /// Root delay (seconds)
    pub root_delay: f64,
    /// Root dispersion (seconds)
    pub root_dispersion: f64,
    /// Reference identifier
    pub ref_id: [u8; 4],
    /// Reference timestamp
    pub ref_timestamp: NtpTimestamp,
    /// Origin timestamp (T1 - client send time)
    pub orig_timestamp: NtpTimestamp,
    /// Receive timestamp (T2 - server receive time)
    pub recv_timestamp: NtpTimestamp,
    /// Transmit timestamp (T3 - server send time)
    pub xmit_timestamp: NtpTimestamp,
}

impl Default for NtpPacket {
    fn default() -> Self {
        Self {
            leap: 0,
            version: 4,
            mode: 3, // Client mode
            stratum: 0,
            poll: 6,
            precision: -20,
            root_delay: 0.0,
            root_dispersion: 0.0,
            ref_id: [0; 4],
            ref_timestamp: NtpTimestamp::default(),
            orig_timestamp: NtpTimestamp::default(),
            recv_timestamp: NtpTimestamp::default(),
            xmit_timestamp: NtpTimestamp::default(),
        }
    }
}

impl NtpPacket {
    /// Create a client request packet
    pub fn client_request() -> Self {
        let mut packet = Self::default();
        packet.xmit_timestamp = NtpTimestamp::now();
        packet
    }

    /// Serialize packet to bytes
    pub fn to_bytes(&self) -> [u8; NTP_PACKET_SIZE] {
        let mut bytes = [0u8; NTP_PACKET_SIZE];

        // LI, VN, Mode packed into first byte
        bytes[0] = (self.leap << 6) | (self.version << 3) | self.mode;
        bytes[1] = self.stratum;
        bytes[2] = self.poll as u8;
        bytes[3] = self.precision as u8;

        // Root delay (32-bit fixed point, 16.16)
        let root_delay_fixed = (self.root_delay * 65536.0) as i32;
        bytes[4..8].copy_from_slice(&root_delay_fixed.to_be_bytes());

        // Root dispersion (32-bit fixed point, 16.16)
        let root_disp_fixed = (self.root_dispersion * 65536.0) as u32;
        bytes[8..12].copy_from_slice(&root_disp_fixed.to_be_bytes());

        // Reference ID
        bytes[12..16].copy_from_slice(&self.ref_id);

        // Timestamps
        bytes[16..24].copy_from_slice(&self.ref_timestamp.to_bytes());
        bytes[24..32].copy_from_slice(&self.orig_timestamp.to_bytes());
        bytes[32..40].copy_from_slice(&self.recv_timestamp.to_bytes());
        bytes[40..48].copy_from_slice(&self.xmit_timestamp.to_bytes());

        bytes
    }

    /// Parse packet from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < NTP_PACKET_SIZE {
            return Err(anyhow!("Packet too small"));
        }

        let li_vn_mode = bytes[0];
        let leap = (li_vn_mode >> 6) & 0x03;
        let version = (li_vn_mode >> 3) & 0x07;
        let mode = li_vn_mode & 0x07;

        let root_delay_fixed = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let root_disp_fixed = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        Ok(Self {
            leap,
            version,
            mode,
            stratum: bytes[1],
            poll: bytes[2] as i8,
            precision: bytes[3] as i8,
            root_delay: root_delay_fixed as f64 / 65536.0,
            root_dispersion: root_disp_fixed as f64 / 65536.0,
            ref_id: [bytes[12], bytes[13], bytes[14], bytes[15]],
            ref_timestamp: NtpTimestamp::from_bytes(&bytes[16..24]),
            orig_timestamp: NtpTimestamp::from_bytes(&bytes[24..32]),
            recv_timestamp: NtpTimestamp::from_bytes(&bytes[32..40]),
            xmit_timestamp: NtpTimestamp::from_bytes(&bytes[40..48]),
        })
    }
}

/// NTP measurement result
#[derive(Debug, Clone)]
pub struct NtpMeasurement {
    /// Server address
    pub server: String,
    /// Clock offset (seconds) - positive means local clock is behind
    pub offset: f64,
    /// Round-trip delay (seconds)
    pub delay: f64,
    /// Server stratum
    pub stratum: u8,
    /// Measurement timestamp
    pub timestamp: SystemTime,
}

/// NTP client for time synchronization
pub struct NtpClient {
    config: NtpConfig,
    socket: Option<UdpSocket>,
}

impl NtpClient {
    /// Create new NTP client
    pub fn new(config: NtpConfig) -> Self {
        Self {
            config,
            socket: None,
        }
    }

    /// Initialize the UDP socket
    fn init_socket(&mut self) -> Result<()> {
        if self.socket.is_none() {
            let socket = UdpSocket::bind("0.0.0.0:0")?;
            socket.set_read_timeout(Some(Duration::from_secs(5)))?;
            socket.set_write_timeout(Some(Duration::from_secs(5)))?;
            self.socket = Some(socket);
        }
        Ok(())
    }

    /// Query a single NTP server
    pub fn query_server(&mut self, server: &str) -> Result<NtpMeasurement> {
        self.init_socket()?;
        let socket = self.socket.as_ref().unwrap();

        // Resolve server address
        let addr: SocketAddr = format!("{}:{}", server, NTP_PORT)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| anyhow!("Could not resolve server: {}", server))?;

        // Create and send request
        let request = NtpPacket::client_request();
        let t1 = NtpTimestamp::now();
        socket.send_to(&request.to_bytes(), addr)?;

        // Receive response
        let mut buf = [0u8; NTP_PACKET_SIZE];
        let (len, _) = socket.recv_from(&mut buf)?;

        let t4 = NtpTimestamp::now();

        if len < NTP_PACKET_SIZE {
            return Err(anyhow!("Invalid response size"));
        }

        let response = NtpPacket::from_bytes(&buf)?;

        // Validate response
        if response.mode != 4 {
            return Err(anyhow!("Invalid response mode: {}", response.mode));
        }

        if response.stratum == 0 {
            return Err(anyhow!("Server not synchronized (stratum 0)"));
        }

        // Calculate offset and delay using NTP algorithm
        // T1 = client send time (origin)
        // T2 = server receive time
        // T3 = server send time (transmit)
        // T4 = client receive time
        //
        // offset = ((T2 - T1) + (T3 - T4)) / 2
        // delay = (T4 - T1) - (T3 - T2)

        let t1_secs = t1.to_unix_secs();
        let t2_secs = response.recv_timestamp.to_unix_secs();
        let t3_secs = response.xmit_timestamp.to_unix_secs();
        let t4_secs = t4.to_unix_secs();

        let offset = ((t2_secs - t1_secs) + (t3_secs - t4_secs)) / 2.0;
        let delay = (t4_secs - t1_secs) - (t3_secs - t2_secs);

        debug!(
            "NTP {} - offset: {:.6}s, delay: {:.6}s, stratum: {}",
            server, offset, delay, response.stratum
        );

        Ok(NtpMeasurement {
            server: server.to_string(),
            offset,
            delay,
            stratum: response.stratum,
            timestamp: SystemTime::now(),
        })
    }

    /// Query all configured servers and return best result
    pub fn sync(&mut self) -> Result<NtpMeasurement> {
        let mut measurements = Vec::new();
        let mut errors = Vec::new();

        for server in &self.config.servers.clone() {
            match self.query_server(server) {
                Ok(measurement) => {
                    measurements.push(measurement);
                }
                Err(e) => {
                    warn!("Failed to query {}: {}", server, e);
                    errors.push((server.clone(), e));
                }
            }
        }

        if measurements.len() < self.config.min_servers {
            return Err(anyhow!(
                "Only {} servers responded, need at least {}",
                measurements.len(),
                self.config.min_servers
            ));
        }

        // Sort by delay and pick the best (lowest delay = most accurate)
        measurements.sort_by(|a, b| a.delay.partial_cmp(&b.delay).unwrap());

        let best = measurements.remove(0);

        // Check panic threshold
        if best.offset.abs() > self.config.panic_threshold {
            return Err(anyhow!(
                "Offset {:.3}s exceeds panic threshold {:.3}s",
                best.offset,
                self.config.panic_threshold
            ));
        }

        info!(
            "NTP sync: offset={:.6}s delay={:.6}s server={}",
            best.offset, best.delay, best.server
        );

        Ok(best)
    }

    /// Check if offset requires step correction vs slew
    pub fn needs_step(&self, offset: f64) -> bool {
        offset.abs() > self.config.step_threshold
    }
}

/// Synchronized time state
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    /// Last sync timestamp
    pub last_sync: Option<SystemTime>,
    /// Last measured offset
    pub last_offset: f64,
    /// Last measured delay
    pub last_delay: f64,
    /// Current stratum (ours = best server + 1)
    pub stratum: u8,
    /// Is synchronized
    pub synchronized: bool,
    /// Reference server
    pub ref_server: Option<String>,
    /// Number of successful syncs
    pub sync_count: u64,
    /// Number of failed syncs
    pub fail_count: u64,
}

impl SyncState {
    /// Update state from measurement
    pub fn update(&mut self, measurement: &NtpMeasurement) {
        self.last_sync = Some(measurement.timestamp);
        self.last_offset = measurement.offset;
        self.last_delay = measurement.delay;
        self.stratum = measurement.stratum.saturating_add(1);
        self.synchronized = true;
        self.ref_server = Some(measurement.server.clone());
        self.sync_count += 1;
    }

    /// Mark sync failure
    pub fn fail(&mut self) {
        self.fail_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntp_timestamp_roundtrip() {
        let ts = NtpTimestamp::now();
        let bytes = ts.to_bytes();
        let ts2 = NtpTimestamp::from_bytes(&bytes);
        assert_eq!(ts.seconds, ts2.seconds);
        assert_eq!(ts.fraction, ts2.fraction);
    }

    #[test]
    fn test_ntp_packet_roundtrip() {
        let packet = NtpPacket::client_request();
        let bytes = packet.to_bytes();
        let packet2 = NtpPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.version, packet2.version);
        assert_eq!(packet.mode, packet2.mode);
    }
}
