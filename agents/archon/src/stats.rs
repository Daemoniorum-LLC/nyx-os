//! Statistics collection
//!
//! Collects and aggregates process and system statistics.

use crate::config::{MetricType, StatsConfig};
use crate::process::{ProcessInfo, ProcessManager, ProcessState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Process statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStats {
    /// Process ID
    pub id: uuid::Uuid,
    /// System PID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// CPU usage percentage
    pub cpu_percent: f32,
    /// Memory usage (bytes)
    pub memory_bytes: u64,
    /// Memory usage percentage
    pub memory_percent: f32,
    /// Number of threads
    pub num_threads: u32,
    /// IO read bytes
    pub io_read_bytes: u64,
    /// IO write bytes
    pub io_write_bytes: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// System-wide statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// CPU usage percentage
    pub cpu_percent: f32,
    /// Memory used (bytes)
    pub memory_used: u64,
    /// Memory total (bytes)
    pub memory_total: u64,
    /// Memory usage percentage
    pub memory_percent: f32,
    /// Total processes managed
    pub process_count: u64,
    /// Active processes
    pub active_processes: u64,
    /// Load average (1, 5, 15 minutes)
    pub load_average: [f32; 3],
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSnapshot {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// System stats
    pub system: SystemStats,
    /// Per-process stats
    pub processes: Vec<ProcessStats>,
}

/// Statistics collector
pub struct StatsCollector {
    /// Configuration
    config: StatsConfig,
    /// Process manager reference
    process_manager: Arc<RwLock<ProcessManager>>,
    /// History of snapshots
    history: RwLock<VecDeque<StatsSnapshot>>,
    /// Previous CPU times for delta calculation
    prev_cpu_times: RwLock<std::collections::HashMap<u32, (u64, u64)>>,
    /// Previous system CPU time
    prev_system_cpu: RwLock<(u64, u64, u64)>, // user, system, idle
}

impl StatsCollector {
    /// Create a new stats collector
    pub fn new(
        config: &StatsConfig,
        process_manager: Arc<RwLock<ProcessManager>>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            config: config.clone(),
            process_manager,
            history: RwLock::new(VecDeque::with_capacity(config.history_size)),
            prev_cpu_times: RwLock::new(std::collections::HashMap::new()),
            prev_system_cpu: RwLock::new((0, 0, 0)),
        })
    }

    /// Collect a statistics snapshot
    pub async fn collect(&self) -> StatsSnapshot {
        let timestamp = Utc::now();

        // Collect system stats
        let system = self.collect_system_stats().await;

        // Collect process stats
        let processes = self.collect_process_stats().await;

        let snapshot = StatsSnapshot {
            timestamp,
            system,
            processes,
        };

        // Store in history
        let mut history = self.history.write().await;
        history.push_back(snapshot.clone());
        while history.len() > self.config.history_size {
            history.pop_front();
        }

        snapshot
    }

    async fn collect_system_stats(&self) -> SystemStats {
        let timestamp = Utc::now();

        // Read /proc/stat for CPU
        let (cpu_percent, new_cpu) = if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
            let cpu_line = stat.lines().next().unwrap_or("");
            let parts: Vec<u64> = cpu_line
                .split_whitespace()
                .skip(1)
                .filter_map(|s| s.parse().ok())
                .collect();

            let user = parts.get(0).copied().unwrap_or(0);
            let system = parts.get(2).copied().unwrap_or(0);
            let idle = parts.get(3).copied().unwrap_or(0);

            let prev = *self.prev_system_cpu.read().await;
            let delta_user = user.saturating_sub(prev.0);
            let delta_system = system.saturating_sub(prev.1);
            let delta_idle = idle.saturating_sub(prev.2);
            let delta_total = delta_user + delta_system + delta_idle;

            let percent = if delta_total > 0 {
                ((delta_user + delta_system) as f32 / delta_total as f32) * 100.0
            } else {
                0.0
            };

            (percent, (user, system, idle))
        } else {
            (0.0, (0, 0, 0))
        };

        *self.prev_system_cpu.write().await = new_cpu;

        // Read /proc/meminfo
        let (memory_total, memory_used, memory_percent) = if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let mut total = 0u64;
            let mut available = 0u64;

            for line in meminfo.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let value: u64 = parts[1].parse().unwrap_or(0) * 1024;
                    match parts[0] {
                        "MemTotal:" => total = value,
                        "MemAvailable:" => available = value,
                        _ => {}
                    }
                }
            }

            let used = total.saturating_sub(available);
            let percent = if total > 0 {
                (used as f32 / total as f32) * 100.0
            } else {
                0.0
            };

            (total, used, percent)
        } else {
            (0, 0, 0.0)
        };

        // Read /proc/loadavg
        let load_average = if let Ok(loadavg) = std::fs::read_to_string("/proc/loadavg") {
            let parts: Vec<f32> = loadavg
                .split_whitespace()
                .take(3)
                .filter_map(|s| s.parse().ok())
                .collect();

            [
                parts.get(0).copied().unwrap_or(0.0),
                parts.get(1).copied().unwrap_or(0.0),
                parts.get(2).copied().unwrap_or(0.0),
            ]
        } else {
            [0.0, 0.0, 0.0]
        };

        // Get process counts
        let pm = self.process_manager.read().await;
        let process_count = pm.count();
        let active_processes = pm.active_count() as u64;

        SystemStats {
            cpu_percent,
            memory_used,
            memory_total,
            memory_percent,
            process_count,
            active_processes,
            load_average,
            timestamp,
        }
    }

    async fn collect_process_stats(&self) -> Vec<ProcessStats> {
        let pm = self.process_manager.read().await;
        let processes = pm.list();
        let mut stats = Vec::with_capacity(processes.len());

        let mut prev_times = self.prev_cpu_times.write().await;

        // Get total system memory for percentage calculation
        let memory_total = if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            meminfo
                .lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u64>().ok())
                .map(|kb| kb * 1024)
                .unwrap_or(1)
        } else {
            1
        };

        for process in processes {
            if process.state != ProcessState::Running {
                continue;
            }

            let pid = process.pid;

            // Read /proc/[pid]/stat
            let stat_path = format!("/proc/{}/stat", pid);
            let statm_path = format!("/proc/{}/statm", pid);
            let io_path = format!("/proc/{}/io", pid);

            let (cpu_percent, utime, stime) = if let Ok(stat) = std::fs::read_to_string(&stat_path) {
                let parts: Vec<&str> = stat.split_whitespace().collect();
                let utime: u64 = parts.get(13).and_then(|s| s.parse().ok()).unwrap_or(0);
                let stime: u64 = parts.get(14).and_then(|s| s.parse().ok()).unwrap_or(0);

                let prev = prev_times.get(&pid).copied().unwrap_or((0, 0));
                let delta = (utime + stime).saturating_sub(prev.0 + prev.1);

                // Rough CPU percentage (would need proper time delta calculation)
                let percent = (delta as f32 / 100.0).min(100.0);

                (percent, utime, stime)
            } else {
                (0.0, 0, 0)
            };

            prev_times.insert(pid, (utime, stime));

            let (memory_bytes, num_threads) = if let Ok(stat) = std::fs::read_to_string(&stat_path) {
                let parts: Vec<&str> = stat.split_whitespace().collect();
                let rss_pages: u64 = parts.get(23).and_then(|s| s.parse().ok()).unwrap_or(0);
                let num_threads: u32 = parts.get(19).and_then(|s| s.parse().ok()).unwrap_or(1);
                (rss_pages * 4096, num_threads) // 4KB pages
            } else {
                (0, 1)
            };

            let (io_read_bytes, io_write_bytes) = if let Ok(io) = std::fs::read_to_string(&io_path) {
                let mut read = 0u64;
                let mut write = 0u64;
                for line in io.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        match parts[0] {
                            "read_bytes:" => read = parts[1].parse().unwrap_or(0),
                            "write_bytes:" => write = parts[1].parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                (read, write)
            } else {
                (0, 0)
            };

            let memory_percent = (memory_bytes as f32 / memory_total as f32) * 100.0;

            stats.push(ProcessStats {
                id: process.id,
                pid,
                name: process.name.clone(),
                cpu_percent,
                memory_bytes,
                memory_percent,
                num_threads,
                io_read_bytes,
                io_write_bytes,
                timestamp: Utc::now(),
            });
        }

        stats
    }

    /// Get latest snapshot
    pub async fn latest(&self) -> Option<StatsSnapshot> {
        let history = self.history.read().await;
        history.back().cloned()
    }

    /// Get history
    pub async fn history(&self, limit: usize) -> Vec<StatsSnapshot> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get stats for a specific process
    pub async fn process_history(&self, id: uuid::Uuid, limit: usize) -> Vec<ProcessStats> {
        let history = self.history.read().await;
        history
            .iter()
            .rev()
            .take(limit)
            .flat_map(|s| s.processes.iter().filter(|p| p.id == id).cloned())
            .collect()
    }

    /// Export metrics to file
    pub async fn export(&self) -> anyhow::Result<()> {
        if let Some(ref path) = self.config.export_path {
            let history = self.history.read().await;
            let json = serde_json::to_string_pretty(&*history)?;
            std::fs::write(path, json)?;
            debug!("Exported metrics to {}", path);
        }
        Ok(())
    }

    /// Get aggregate statistics
    pub async fn aggregate(&self, duration_secs: u64) -> AggregateStats {
        let history = self.history.read().await;
        let cutoff = Utc::now() - chrono::Duration::seconds(duration_secs as i64);

        let recent: Vec<_> = history
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();

        if recent.is_empty() {
            return AggregateStats::default();
        }

        let count = recent.len() as f32;

        let avg_cpu = recent.iter().map(|s| s.system.cpu_percent).sum::<f32>() / count;
        let max_cpu = recent.iter().map(|s| s.system.cpu_percent).fold(0.0f32, f32::max);

        let avg_memory = recent.iter().map(|s| s.system.memory_percent).sum::<f32>() / count;
        let max_memory = recent.iter().map(|s| s.system.memory_percent).fold(0.0f32, f32::max);

        let avg_processes = recent.iter().map(|s| s.system.active_processes as f32).sum::<f32>() / count;

        AggregateStats {
            duration_secs,
            samples: recent.len(),
            avg_cpu_percent: avg_cpu,
            max_cpu_percent: max_cpu,
            avg_memory_percent: avg_memory,
            max_memory_percent: max_memory,
            avg_active_processes: avg_processes,
        }
    }
}

/// Aggregate statistics over a time period
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregateStats {
    /// Time period in seconds
    pub duration_secs: u64,
    /// Number of samples
    pub samples: usize,
    /// Average CPU usage
    pub avg_cpu_percent: f32,
    /// Maximum CPU usage
    pub max_cpu_percent: f32,
    /// Average memory usage
    pub avg_memory_percent: f32,
    /// Maximum memory usage
    pub max_memory_percent: f32,
    /// Average active processes
    pub avg_active_processes: f32,
}
