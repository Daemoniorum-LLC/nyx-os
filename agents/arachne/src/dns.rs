//! DNS resolution and caching

use crate::config::DnsConfig;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// DNS resolver with caching
pub struct DnsResolver {
    config: DnsConfig,
    cache: Arc<RwLock<DnsCache>>,
    blocklist: Arc<RwLock<Vec<String>>>,
}

struct DnsCache {
    entries: HashMap<String, CacheEntry>,
    max_size: usize,
}

#[derive(Clone)]
struct CacheEntry {
    records: Vec<DnsRecord>,
    expires: Instant,
    hits: u64,
}

#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub record_type: RecordType,
    pub name: String,
    pub value: String,
    pub ttl: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
    PTR,
    SRV,
    SOA,
}

impl DnsResolver {
    pub fn new(config: DnsConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(DnsCache {
                entries: HashMap::new(),
                max_size: config.cache_size,
            })),
            blocklist: Arc::new(RwLock::new(config.blocklist.clone())),
            config,
        }
    }

    /// Resolve a hostname to IP addresses
    pub async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>> {
        // Check blocklist
        if self.is_blocked(hostname).await {
            tracing::warn!("Blocked DNS query for: {}", hostname);
            return Err(anyhow!("Domain blocked: {}", hostname));
        }

        // Check cache
        if self.config.cache_enabled {
            if let Some(cached) = self.get_cached(hostname).await {
                return Ok(cached);
            }
        }

        // Query upstream
        let records = self.query_upstream(hostname, RecordType::A).await?;

        // Extract IPs
        let ips: Vec<IpAddr> = records
            .iter()
            .filter_map(|r| r.value.parse().ok())
            .collect();

        // Cache results
        if self.config.cache_enabled && !ips.is_empty() {
            let ttl = records.first().map(|r| r.ttl).unwrap_or(300);
            self.cache_result(hostname, records).await;
        }

        Ok(ips)
    }

    /// Resolve with specific record type
    pub async fn resolve_type(&self, hostname: &str, record_type: RecordType) -> Result<Vec<DnsRecord>> {
        if self.is_blocked(hostname).await {
            return Err(anyhow!("Domain blocked: {}", hostname));
        }

        self.query_upstream(hostname, record_type).await
    }

    /// Reverse DNS lookup
    pub async fn reverse_lookup(&self, ip: &IpAddr) -> Result<String> {
        let ptr_name = match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!("{}.{}.{}.{}.in-addr.arpa",
                    octets[3], octets[2], octets[1], octets[0])
            }
            IpAddr::V6(v6) => {
                // IPv6 reverse DNS
                let segments = v6.segments();
                let mut name = String::new();
                for seg in segments.iter().rev() {
                    for nibble in [
                        (seg & 0x000F) as u8,
                        ((seg >> 4) & 0x000F) as u8,
                        ((seg >> 8) & 0x000F) as u8,
                        ((seg >> 12) & 0x000F) as u8,
                    ] {
                        name.push_str(&format!("{:x}.", nibble));
                    }
                }
                name.push_str("ip6.arpa");
                name
            }
        };

        let records = self.query_upstream(&ptr_name, RecordType::PTR).await?;
        records.first()
            .map(|r| r.value.clone())
            .ok_or_else(|| anyhow!("No PTR record found"))
    }

    async fn query_upstream(&self, hostname: &str, record_type: RecordType) -> Result<Vec<DnsRecord>> {
        for upstream in &self.config.upstream {
            match self.query_server(upstream, hostname, record_type).await {
                Ok(records) if !records.is_empty() => return Ok(records),
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!("DNS query to {} failed: {}", upstream, e);
                    continue;
                }
            }
        }

        Err(anyhow!("All upstream DNS servers failed"))
    }

    async fn query_server(
        &self,
        server: &str,
        hostname: &str,
        record_type: RecordType,
    ) -> Result<Vec<DnsRecord>> {
        // Build DNS query packet
        let query = build_dns_query(hostname, record_type)?;

        // Send query
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;

        let server_addr: SocketAddr = format!("{}:53", server).parse()?;
        socket.send_to(&query, server_addr)?;

        // Receive response
        let mut buf = vec![0u8; 512];
        let (len, _) = socket.recv_from(&mut buf)?;
        buf.truncate(len);

        // Parse response
        parse_dns_response(&buf)
    }

    async fn is_blocked(&self, hostname: &str) -> bool {
        let blocklist = self.blocklist.read().await;

        for pattern in blocklist.iter() {
            if pattern.starts_with("*.") {
                // Wildcard match
                let suffix = &pattern[1..];
                if hostname.ends_with(suffix) {
                    return true;
                }
            } else if hostname == pattern || hostname.ends_with(&format!(".{}", pattern)) {
                return true;
            }
        }

        false
    }

    async fn get_cached(&self, hostname: &str) -> Option<Vec<IpAddr>> {
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.entries.get_mut(hostname) {
            if entry.expires > Instant::now() {
                entry.hits += 1;
                return Some(
                    entry.records
                        .iter()
                        .filter_map(|r| r.value.parse().ok())
                        .collect()
                );
            } else {
                // Expired
                cache.entries.remove(hostname);
            }
        }

        None
    }

    async fn cache_result(&self, hostname: &str, records: Vec<DnsRecord>) {
        let mut cache = self.cache.write().await;

        // Evict if full
        if cache.entries.len() >= cache.max_size {
            // Remove least-hit entry
            if let Some(key) = cache.entries
                .iter()
                .min_by_key(|(_, v)| v.hits)
                .map(|(k, _)| k.clone())
            {
                cache.entries.remove(&key);
            }
        }

        let ttl = records.first().map(|r| r.ttl).unwrap_or(300);
        cache.entries.insert(hostname.to_string(), CacheEntry {
            records,
            expires: Instant::now() + Duration::from_secs(ttl as u64),
            hits: 0,
        });
    }

    /// Add domain to blocklist
    pub async fn block_domain(&self, domain: &str) {
        self.blocklist.write().await.push(domain.to_string());
        tracing::info!("Added {} to DNS blocklist", domain);
    }

    /// Remove domain from blocklist
    pub async fn unblock_domain(&self, domain: &str) {
        self.blocklist.write().await.retain(|d| d != domain);
    }

    /// Clear DNS cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.entries.clear();
        tracing::info!("DNS cache cleared");
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> DnsStats {
        let cache = self.cache.read().await;
        let total_hits: u64 = cache.entries.values().map(|e| e.hits).sum();

        DnsStats {
            cache_size: cache.entries.len(),
            cache_max: cache.max_size,
            total_hits,
            blocklist_size: self.blocklist.read().await.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsStats {
    pub cache_size: usize,
    pub cache_max: usize,
    pub total_hits: u64,
    pub blocklist_size: usize,
}

/// Build a DNS query packet
fn build_dns_query(hostname: &str, record_type: RecordType) -> Result<Vec<u8>> {
    let mut packet = Vec::new();

    // Transaction ID
    packet.extend_from_slice(&rand::random::<[u8; 2]>());

    // Flags: standard query
    packet.extend_from_slice(&[0x01, 0x00]);

    // Questions: 1, Answers: 0, Authority: 0, Additional: 0
    packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Question section
    for label in hostname.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0x00); // End of name

    // Record type
    let qtype = match record_type {
        RecordType::A => 1,
        RecordType::AAAA => 28,
        RecordType::CNAME => 5,
        RecordType::MX => 15,
        RecordType::TXT => 16,
        RecordType::NS => 2,
        RecordType::PTR => 12,
        RecordType::SRV => 33,
        RecordType::SOA => 6,
    };
    packet.extend_from_slice(&(qtype as u16).to_be_bytes());

    // Class: IN
    packet.extend_from_slice(&[0x00, 0x01]);

    Ok(packet)
}

/// Parse DNS response packet
fn parse_dns_response(packet: &[u8]) -> Result<Vec<DnsRecord>> {
    if packet.len() < 12 {
        return Err(anyhow!("Invalid DNS response"));
    }

    let mut records = Vec::new();

    // Parse header
    let _id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let _qd_count = u16::from_be_bytes([packet[4], packet[5]]);
    let an_count = u16::from_be_bytes([packet[6], packet[7]]);

    // Check for errors
    let rcode = flags & 0x000F;
    if rcode != 0 {
        return Err(anyhow!("DNS error: {}", rcode));
    }

    // Skip question section
    let mut pos = 12;
    while pos < packet.len() && packet[pos] != 0 {
        let len = packet[pos] as usize;
        pos += len + 1;
    }
    pos += 5; // Skip null byte, type, and class

    // Parse answer section
    for _ in 0..an_count {
        if pos >= packet.len() {
            break;
        }

        // Parse name (might be compressed)
        let (name, new_pos) = parse_dns_name(packet, pos)?;
        pos = new_pos;

        if pos + 10 > packet.len() {
            break;
        }

        let rtype = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
        let _rclass = u16::from_be_bytes([packet[pos + 2], packet[pos + 3]]);
        let ttl = u32::from_be_bytes([packet[pos + 4], packet[pos + 5], packet[pos + 6], packet[pos + 7]]);
        let rdlength = u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]) as usize;
        pos += 10;

        if pos + rdlength > packet.len() {
            break;
        }

        let rdata = &packet[pos..pos + rdlength];
        pos += rdlength;

        // Parse record value based on type
        let (record_type, value) = match rtype {
            1 if rdlength == 4 => {
                // A record
                (RecordType::A, format!("{}.{}.{}.{}", rdata[0], rdata[1], rdata[2], rdata[3]))
            }
            28 if rdlength == 16 => {
                // AAAA record
                let addr: [u8; 16] = rdata.try_into().unwrap();
                let ip = std::net::Ipv6Addr::from(addr);
                (RecordType::AAAA, ip.to_string())
            }
            5 => {
                // CNAME
                let (cname, _) = parse_dns_name(packet, pos - rdlength)?;
                (RecordType::CNAME, cname)
            }
            12 => {
                // PTR
                let (ptr, _) = parse_dns_name(packet, pos - rdlength)?;
                (RecordType::PTR, ptr)
            }
            _ => continue,
        };

        records.push(DnsRecord {
            record_type,
            name,
            value,
            ttl,
        });
    }

    Ok(records)
}

fn parse_dns_name(packet: &[u8], mut pos: usize) -> Result<(String, usize)> {
    let mut name = String::new();
    let mut jumped = false;
    let original_pos = pos;

    loop {
        if pos >= packet.len() {
            break;
        }

        let len = packet[pos] as usize;

        if len == 0 {
            pos += 1;
            break;
        }

        // Compression pointer
        if len & 0xC0 == 0xC0 {
            if pos + 1 >= packet.len() {
                break;
            }
            let offset = ((len & 0x3F) << 8) | (packet[pos + 1] as usize);
            if !jumped {
                pos += 2;
            }
            jumped = true;
            let (rest, _) = parse_dns_name(packet, offset)?;
            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(&rest);
            break;
        }

        if pos + 1 + len > packet.len() {
            break;
        }

        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(&packet[pos + 1..pos + 1 + len]));
        pos += 1 + len;
    }

    let final_pos = if jumped { original_pos + 2 } else { pos };
    Ok((name, final_pos))
}
