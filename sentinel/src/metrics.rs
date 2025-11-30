//! System metrics collection

use crate::config::MetricsConfig;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use sysinfo::{Components, Disks, Networks, RefreshKind, System};

/// CPU metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Overall CPU usage percentage
    pub usage: f32,
    /// Per-core usage
    pub cores: Vec<f32>,
    /// Number of physical cores
    pub physical_cores: usize,
    /// Number of logical cores
    pub logical_cores: usize,
    /// CPU frequency (MHz)
    pub frequency: u64,
    /// CPU brand/model
    pub brand: String,
}

/// Memory metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Total memory in bytes
    pub total: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Free memory in bytes
    pub free: u64,
    /// Available memory in bytes
    pub available: u64,
    /// Usage percentage
    pub usage_percent: f32,
    /// Swap total
    pub swap_total: u64,
    /// Swap used
    pub swap_used: u64,
    /// Swap free
    pub swap_free: u64,
}

/// Disk metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    /// Disk name/device
    pub name: String,
    /// Mount point
    pub mount_point: String,
    /// File system type
    pub fs_type: String,
    /// Total space in bytes
    pub total: u64,
    /// Used space in bytes
    pub used: u64,
    /// Available space in bytes
    pub available: u64,
    /// Usage percentage
    pub usage_percent: f32,
}

/// Network interface metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Interface name
    pub name: String,
    /// Bytes received
    pub rx_bytes: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Errors on receive
    pub rx_errors: u64,
    /// Errors on transmit
    pub tx_errors: u64,
}

/// Temperature sensor metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureMetrics {
    /// Sensor label
    pub label: String,
    /// Current temperature (Celsius)
    pub temperature: f32,
    /// Critical temperature (Celsius)
    pub critical: Option<f32>,
}

/// Process metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// CPU usage percentage
    pub cpu_usage: f32,
    /// Memory usage in bytes
    pub memory: u64,
    /// Virtual memory in bytes
    pub virtual_memory: u64,
    /// Process status
    pub status: String,
    /// Run time in seconds
    pub run_time: u64,
}

/// Load average
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    /// 1-minute load average
    pub one: f64,
    /// 5-minute load average
    pub five: f64,
    /// 15-minute load average
    pub fifteen: f64,
}

/// System uptime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uptime {
    /// Total seconds
    pub seconds: u64,
    /// Days
    pub days: u64,
    /// Hours
    pub hours: u64,
    /// Minutes
    pub minutes: u64,
}

/// Complete system snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// CPU metrics
    pub cpu: Option<CpuMetrics>,
    /// Memory metrics
    pub memory: Option<MemoryMetrics>,
    /// Disk metrics
    pub disks: Vec<DiskMetrics>,
    /// Network metrics
    pub networks: Vec<NetworkMetrics>,
    /// Temperature metrics
    pub temperatures: Vec<TemperatureMetrics>,
    /// Top processes by CPU
    pub top_cpu_processes: Vec<ProcessMetrics>,
    /// Top processes by memory
    pub top_memory_processes: Vec<ProcessMetrics>,
    /// Load average
    pub load: LoadAverage,
    /// System uptime
    pub uptime: Uptime,
}

/// Metrics collector
pub struct MetricsCollector {
    config: MetricsConfig,
    system: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    history: VecDeque<SystemSnapshot>,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        let refresh = RefreshKind::everything();

        Self {
            config,
            system: System::new_with_specifics(refresh),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            history: VecDeque::new(),
        }
    }

    /// Collect current system metrics
    pub fn collect(&mut self) -> SystemSnapshot {
        // Refresh system information
        self.system.refresh_all();
        self.disks.refresh();
        self.networks.refresh();
        self.components.refresh();

        let cpu = if self.config.cpu {
            Some(self.collect_cpu())
        } else {
            None
        };

        let memory = if self.config.memory {
            Some(self.collect_memory())
        } else {
            None
        };

        let disks = if self.config.disk {
            self.collect_disks()
        } else {
            Vec::new()
        };

        let networks = if self.config.network {
            self.collect_networks()
        } else {
            Vec::new()
        };

        let temperatures = if self.config.temperature {
            self.collect_temperatures()
        } else {
            Vec::new()
        };

        let (top_cpu_processes, top_memory_processes) = if self.config.processes {
            self.collect_processes()
        } else {
            (Vec::new(), Vec::new())
        };

        let load = self.collect_load();
        let uptime = self.collect_uptime();

        let snapshot = SystemSnapshot {
            timestamp: chrono::Utc::now(),
            cpu,
            memory,
            disks,
            networks,
            temperatures,
            top_cpu_processes,
            top_memory_processes,
            load,
            uptime,
        };

        // Add to history
        self.history.push_back(snapshot.clone());
        while self.history.len() > self.config.history_size {
            self.history.pop_front();
        }

        snapshot
    }

    /// Collect CPU metrics
    fn collect_cpu(&self) -> CpuMetrics {
        let cpus = self.system.cpus();

        let usage = self.system.global_cpu_usage();
        let cores: Vec<f32> = cpus.iter().map(|cpu| cpu.cpu_usage()).collect();
        let physical_cores = self.system.physical_core_count().unwrap_or(1);
        let logical_cores = cpus.len();
        let frequency = cpus.first().map(|c| c.frequency()).unwrap_or(0);
        let brand = cpus
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default();

        CpuMetrics {
            usage,
            cores,
            physical_cores,
            logical_cores,
            frequency,
            brand,
        }
    }

    /// Collect memory metrics
    fn collect_memory(&self) -> MemoryMetrics {
        let total = self.system.total_memory();
        let used = self.system.used_memory();
        let free = self.system.free_memory();
        let available = self.system.available_memory();
        let swap_total = self.system.total_swap();
        let swap_used = self.system.used_swap();
        let swap_free = self.system.free_swap();

        let usage_percent = if total > 0 {
            (used as f64 / total as f64 * 100.0) as f32
        } else {
            0.0
        };

        MemoryMetrics {
            total,
            used,
            free,
            available,
            usage_percent,
            swap_total,
            swap_used,
            swap_free,
        }
    }

    /// Collect disk metrics
    fn collect_disks(&self) -> Vec<DiskMetrics> {
        self.disks
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);
                let usage_percent = if total > 0 {
                    (used as f64 / total as f64 * 100.0) as f32
                } else {
                    0.0
                };

                DiskMetrics {
                    name: disk.name().to_string_lossy().to_string(),
                    mount_point: disk.mount_point().to_string_lossy().to_string(),
                    fs_type: disk.file_system().to_string_lossy().to_string(),
                    total,
                    used,
                    available,
                    usage_percent,
                }
            })
            .collect()
    }

    /// Collect network metrics
    fn collect_networks(&self) -> Vec<NetworkMetrics> {
        self.networks
            .iter()
            .map(|(name, data)| NetworkMetrics {
                name: name.clone(),
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
                rx_packets: data.total_packets_received(),
                tx_packets: data.total_packets_transmitted(),
                rx_errors: data.total_errors_on_received(),
                tx_errors: data.total_errors_on_transmitted(),
            })
            .collect()
    }

    /// Collect temperature metrics
    fn collect_temperatures(&self) -> Vec<TemperatureMetrics> {
        self.components
            .iter()
            .map(|comp| TemperatureMetrics {
                label: comp.label().to_string(),
                temperature: comp.temperature(),
                critical: comp.critical(),
            })
            .collect()
    }

    /// Collect process metrics
    fn collect_processes(&mut self) -> (Vec<ProcessMetrics>, Vec<ProcessMetrics>) {
        // Processes are already refreshed by refresh_all() in collect()
        let mut processes: Vec<_> = self
            .system
            .processes()
            .iter()
            .map(|(pid, proc)| ProcessMetrics {
                pid: pid.as_u32(),
                name: proc.name().to_string_lossy().to_string(),
                cpu_usage: proc.cpu_usage(),
                memory: proc.memory(),
                virtual_memory: proc.virtual_memory(),
                status: format!("{:?}", proc.status()),
                run_time: proc.run_time(),
            })
            .collect();

        // Top by CPU
        processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
        let top_cpu: Vec<_> = processes
            .iter()
            .take(self.config.top_cpu_count)
            .cloned()
            .collect();

        // Top by memory
        processes.sort_by(|a, b| b.memory.cmp(&a.memory));
        let top_memory: Vec<_> = processes
            .iter()
            .take(self.config.top_memory_count)
            .cloned()
            .collect();

        (top_cpu, top_memory)
    }

    /// Collect load average
    fn collect_load(&self) -> LoadAverage {
        let load = System::load_average();
        LoadAverage {
            one: load.one,
            five: load.five,
            fifteen: load.fifteen,
        }
    }

    /// Collect uptime
    fn collect_uptime(&self) -> Uptime {
        let seconds = System::uptime();
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        let minutes = (seconds % 3600) / 60;

        Uptime {
            seconds,
            days,
            hours,
            minutes,
        }
    }

    /// Get history
    pub fn get_history(&self) -> &VecDeque<SystemSnapshot> {
        &self.history
    }

    /// Get latest snapshot
    pub fn latest(&self) -> Option<&SystemSnapshot> {
        self.history.back()
    }
}
