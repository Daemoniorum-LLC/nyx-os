//! Sandbox configuration and enforcement
//!
//! Guardian can instruct the kernel to run processes in sandboxes with
//! restricted capabilities based on risk assessment.

use crate::config::RiskLevel;
use crate::decision::SandboxLevel;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{debug, info};

/// Sandbox profile - defines restrictions for a sandboxed process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfile {
    /// Profile name
    pub name: String,
    /// Sandbox level
    pub level: SandboxLevel,
    /// Filesystem restrictions
    pub filesystem: FilesystemPolicy,
    /// Network restrictions
    pub network: NetworkPolicy,
    /// Process restrictions
    pub process: ProcessPolicy,
    /// IPC restrictions
    pub ipc: IpcPolicy,
    /// Resource limits
    pub resources: ResourceLimits,
    /// Allowed capabilities
    pub allowed_capabilities: Vec<String>,
    /// Denied capabilities (explicit blocklist)
    pub denied_capabilities: Vec<String>,
}

/// Filesystem access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Allow any filesystem access
    pub enabled: bool,
    /// Read-only paths
    pub read_only: Vec<PathBuf>,
    /// Read-write paths
    pub read_write: Vec<PathBuf>,
    /// Completely hidden paths
    pub hidden: Vec<PathBuf>,
    /// Temporary directory (private tmpfs)
    pub private_tmp: bool,
    /// Private /dev
    pub private_dev: bool,
    /// No access to user home
    pub protect_home: bool,
    /// No access to system paths
    pub protect_system: bool,
}

/// Network access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allow any network access
    pub enabled: bool,
    /// Allow outbound connections
    pub allow_outbound: bool,
    /// Allow inbound connections
    pub allow_inbound: bool,
    /// Allowed destination hosts/IPs
    pub allowed_hosts: Vec<String>,
    /// Allowed destination ports
    pub allowed_ports: Vec<u16>,
    /// Blocked hosts
    pub blocked_hosts: Vec<String>,
    /// Use network namespace
    pub private_network: bool,
    /// DNS policy
    pub dns: DnsPolicy,
}

/// DNS policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsPolicy {
    /// Allow all DNS
    Allow,
    /// Only system resolver
    SystemOnly,
    /// Specific DNS servers
    Specific(Vec<String>),
    /// No DNS
    None,
}

/// Process restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPolicy {
    /// Allow spawning child processes
    pub allow_spawn: bool,
    /// Allowed executables
    pub allowed_executables: Vec<PathBuf>,
    /// No new privileges
    pub no_new_privs: bool,
    /// PID namespace isolation
    pub private_pid: bool,
    /// User namespace mapping
    pub user_namespace: Option<UserMapping>,
}

/// User namespace mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMapping {
    /// Map to this UID inside container
    pub uid: u32,
    /// Map to this GID inside container
    pub gid: u32,
}

/// IPC restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcPolicy {
    /// Allow System V IPC
    pub allow_sysv: bool,
    /// Allow POSIX message queues
    pub allow_mqueue: bool,
    /// Private IPC namespace
    pub private_ipc: bool,
    /// Allowed shared memory segments
    pub allowed_shm: Vec<String>,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU quota (percentage, 0 = unlimited)
    pub cpu_percent: u32,
    /// Memory limit in bytes (0 = unlimited)
    pub memory_bytes: u64,
    /// Maximum open files
    pub max_files: u32,
    /// Maximum processes
    pub max_processes: u32,
    /// Maximum file size
    pub max_file_size: u64,
    /// IO bandwidth limit (bytes/sec, 0 = unlimited)
    pub io_bandwidth: u64,
}

impl Default for SandboxProfile {
    fn default() -> Self {
        Self {
            name: "default".into(),
            level: SandboxLevel::Medium,
            filesystem: FilesystemPolicy::default(),
            network: NetworkPolicy::default(),
            process: ProcessPolicy::default(),
            ipc: IpcPolicy::default(),
            resources: ResourceLimits::default(),
            allowed_capabilities: vec![],
            denied_capabilities: vec![],
        }
    }
}

impl Default for FilesystemPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            read_only: vec![
                PathBuf::from("/usr"),
                PathBuf::from("/lib"),
                PathBuf::from("/lib64"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
            ],
            read_write: vec![],
            hidden: vec![
                PathBuf::from("/root"),
                PathBuf::from("/etc/shadow"),
                PathBuf::from("/etc/sudoers"),
            ],
            private_tmp: true,
            private_dev: true,
            protect_home: false,
            protect_system: true,
        }
    }
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_outbound: true,
            allow_inbound: false,
            allowed_hosts: vec![],
            allowed_ports: vec![80, 443],
            blocked_hosts: vec![],
            private_network: false,
            dns: DnsPolicy::SystemOnly,
        }
    }
}

impl Default for ProcessPolicy {
    fn default() -> Self {
        Self {
            allow_spawn: true,
            allowed_executables: vec![],
            no_new_privs: true,
            private_pid: false,
            user_namespace: None,
        }
    }
}

impl Default for IpcPolicy {
    fn default() -> Self {
        Self {
            allow_sysv: false,
            allow_mqueue: false,
            private_ipc: true,
            allowed_shm: vec![],
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: 100,
            memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            max_files: 1024,
            max_processes: 64,
            max_file_size: 1024 * 1024 * 1024, // 1 GB
            io_bandwidth: 0,                    // unlimited
        }
    }
}

/// Sandbox profile builder
pub struct SandboxBuilder {
    profile: SandboxProfile,
}

impl SandboxBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            profile: SandboxProfile {
                name: name.into(),
                ..Default::default()
            },
        }
    }

    /// Create from sandbox level
    pub fn from_level(level: SandboxLevel) -> Self {
        let mut builder = Self::new(format!("{:?}", level).to_lowercase());
        builder.profile.level = level;

        match level {
            SandboxLevel::Light => builder.light_restrictions(),
            SandboxLevel::Medium => builder.medium_restrictions(),
            SandboxLevel::Heavy => builder.heavy_restrictions(),
            SandboxLevel::Maximum => builder.maximum_restrictions(),
        }
    }

    fn light_restrictions(mut self) -> Self {
        // Light: Allow most things, just basic isolation
        self.profile.filesystem.protect_system = false;
        self.profile.network.allow_inbound = true;
        self.profile.process.private_pid = false;
        self.profile.resources.cpu_percent = 100;
        self.profile.resources.memory_bytes = 8 * 1024 * 1024 * 1024;
        self
    }

    fn medium_restrictions(mut self) -> Self {
        // Medium: Standard sandboxing
        self.profile.filesystem.protect_system = true;
        self.profile.filesystem.protect_home = false;
        self.profile.filesystem.private_tmp = true;
        self.profile.network.allow_inbound = false;
        self.profile.process.no_new_privs = true;
        self.profile.ipc.private_ipc = true;
        self
    }

    fn heavy_restrictions(mut self) -> Self {
        // Heavy: Significant restrictions
        self.profile.filesystem.protect_system = true;
        self.profile.filesystem.protect_home = true;
        self.profile.filesystem.private_tmp = true;
        self.profile.filesystem.private_dev = true;
        self.profile.network.enabled = true;
        self.profile.network.allow_outbound = true;
        self.profile.network.allow_inbound = false;
        self.profile.network.allowed_ports = vec![443]; // HTTPS only
        self.profile.process.no_new_privs = true;
        self.profile.process.private_pid = true;
        self.profile.process.allow_spawn = false;
        self.profile.ipc.private_ipc = true;
        self.profile.ipc.allow_sysv = false;
        self.profile.resources.cpu_percent = 50;
        self.profile.resources.memory_bytes = 2 * 1024 * 1024 * 1024;
        self.profile.resources.max_files = 256;
        self.profile.resources.max_processes = 16;
        self
    }

    fn maximum_restrictions(mut self) -> Self {
        // Maximum: Near-complete isolation
        self.profile.filesystem.enabled = true;
        self.profile.filesystem.protect_system = true;
        self.profile.filesystem.protect_home = true;
        self.profile.filesystem.private_tmp = true;
        self.profile.filesystem.private_dev = true;
        self.profile.filesystem.read_write = vec![]; // Read-only everything
        self.profile.filesystem.hidden = vec![
            PathBuf::from("/root"),
            PathBuf::from("/home"),
            PathBuf::from("/etc"),
            PathBuf::from("/var"),
        ];
        self.profile.network.enabled = false;
        self.profile.network.allow_outbound = false;
        self.profile.network.allow_inbound = false;
        self.profile.network.private_network = true;
        self.profile.network.dns = DnsPolicy::None;
        self.profile.process.no_new_privs = true;
        self.profile.process.private_pid = true;
        self.profile.process.allow_spawn = false;
        self.profile.process.user_namespace = Some(UserMapping {
            uid: 65534,
            gid: 65534,
        });
        self.profile.ipc.private_ipc = true;
        self.profile.ipc.allow_sysv = false;
        self.profile.ipc.allow_mqueue = false;
        self.profile.resources.cpu_percent = 25;
        self.profile.resources.memory_bytes = 512 * 1024 * 1024;
        self.profile.resources.max_files = 64;
        self.profile.resources.max_processes = 4;
        self
    }

    /// Allow specific filesystem paths
    pub fn allow_path(mut self, path: impl Into<PathBuf>, writable: bool) -> Self {
        let path = path.into();
        if writable {
            self.profile.filesystem.read_write.push(path);
        } else {
            self.profile.filesystem.read_only.push(path);
        }
        self
    }

    /// Allow network to specific host
    pub fn allow_host(mut self, host: impl Into<String>) -> Self {
        self.profile.network.allowed_hosts.push(host.into());
        self
    }

    /// Allow specific port
    pub fn allow_port(mut self, port: u16) -> Self {
        self.profile.network.allowed_ports.push(port);
        self
    }

    /// Allow a capability
    pub fn allow_capability(mut self, cap: impl Into<String>) -> Self {
        self.profile.allowed_capabilities.push(cap.into());
        self
    }

    /// Deny a capability
    pub fn deny_capability(mut self, cap: impl Into<String>) -> Self {
        self.profile.denied_capabilities.push(cap.into());
        self
    }

    /// Set memory limit
    pub fn memory_limit(mut self, bytes: u64) -> Self {
        self.profile.resources.memory_bytes = bytes;
        self
    }

    /// Set CPU limit
    pub fn cpu_limit(mut self, percent: u32) -> Self {
        self.profile.resources.cpu_percent = percent;
        self
    }

    /// Build the profile
    pub fn build(self) -> SandboxProfile {
        self.profile
    }
}

/// Sandbox enforcer - applies sandbox profiles to processes
pub struct SandboxEnforcer {
    /// Default profiles by level
    default_profiles: HashMap<SandboxLevel, SandboxProfile>,
    /// Custom profiles by name
    custom_profiles: HashMap<String, SandboxProfile>,
}

impl SandboxEnforcer {
    pub fn new() -> Self {
        let mut default_profiles = HashMap::new();

        // Create default profiles for each level
        default_profiles.insert(
            SandboxLevel::Light,
            SandboxBuilder::from_level(SandboxLevel::Light).build(),
        );
        default_profiles.insert(
            SandboxLevel::Medium,
            SandboxBuilder::from_level(SandboxLevel::Medium).build(),
        );
        default_profiles.insert(
            SandboxLevel::Heavy,
            SandboxBuilder::from_level(SandboxLevel::Heavy).build(),
        );
        default_profiles.insert(
            SandboxLevel::Maximum,
            SandboxBuilder::from_level(SandboxLevel::Maximum).build(),
        );

        Self {
            default_profiles,
            custom_profiles: HashMap::new(),
        }
    }

    /// Register a custom profile
    pub fn register_profile(&mut self, profile: SandboxProfile) {
        info!("Registered sandbox profile: {}", profile.name);
        self.custom_profiles.insert(profile.name.clone(), profile);
    }

    /// Get profile by level
    pub fn get_profile(&self, level: SandboxLevel) -> &SandboxProfile {
        self.default_profiles.get(&level).unwrap()
    }

    /// Get profile by name
    pub fn get_custom_profile(&self, name: &str) -> Option<&SandboxProfile> {
        self.custom_profiles.get(name)
    }

    /// Generate sandbox configuration for kernel
    pub fn generate_config(&self, profile: &SandboxProfile) -> SandboxConfig {
        SandboxConfig {
            namespaces: self.compute_namespaces(profile),
            seccomp: self.generate_seccomp_filter(profile),
            cgroups: self.generate_cgroup_config(profile),
            mounts: self.generate_mount_config(profile),
            capabilities: self.compute_capabilities(profile),
        }
    }

    fn compute_namespaces(&self, profile: &SandboxProfile) -> NamespaceConfig {
        NamespaceConfig {
            user: profile.process.user_namespace.is_some(),
            pid: profile.process.private_pid,
            net: profile.network.private_network,
            ipc: profile.ipc.private_ipc,
            mount: true, // Always use mount namespace for sandbox
            uts: true,
            cgroup: true,
        }
    }

    fn generate_seccomp_filter(&self, profile: &SandboxProfile) -> SeccompConfig {
        let mut allowed_syscalls = HashSet::new();

        // Base syscalls always allowed
        allowed_syscalls.extend([
            "read", "write", "close", "fstat", "mmap", "mprotect",
            "munmap", "brk", "rt_sigaction", "rt_sigprocmask", "exit",
            "exit_group", "nanosleep", "clock_gettime", "getpid", "gettid",
        ]);

        // Filesystem syscalls
        if profile.filesystem.enabled {
            allowed_syscalls.extend([
                "open", "openat", "stat", "lstat", "access", "readlink",
                "getdents", "getdents64", "lseek", "pread64", "pwrite64",
            ]);
        }

        // Network syscalls
        if profile.network.enabled {
            allowed_syscalls.extend([
                "socket", "connect", "bind", "listen", "accept", "accept4",
                "sendto", "recvfrom", "sendmsg", "recvmsg", "getsockopt",
                "setsockopt", "getsockname", "getpeername",
            ]);
        }

        // Process syscalls
        if profile.process.allow_spawn {
            allowed_syscalls.extend([
                "clone", "fork", "vfork", "execve", "execveat", "wait4", "waitid",
            ]);
        }

        SeccompConfig {
            default_action: SeccompAction::Errno(libc::EPERM),
            allowed_syscalls: allowed_syscalls.into_iter().map(String::from).collect(),
            traced_syscalls: vec![],
        }
    }

    fn generate_cgroup_config(&self, profile: &SandboxProfile) -> CgroupConfig {
        CgroupConfig {
            cpu_quota: if profile.resources.cpu_percent > 0 && profile.resources.cpu_percent < 100 {
                Some(profile.resources.cpu_percent * 1000) // Convert to microseconds
            } else {
                None
            },
            memory_max: if profile.resources.memory_bytes > 0 {
                Some(profile.resources.memory_bytes)
            } else {
                None
            },
            pids_max: if profile.resources.max_processes > 0 {
                Some(profile.resources.max_processes)
            } else {
                None
            },
            io_max: if profile.resources.io_bandwidth > 0 {
                Some(profile.resources.io_bandwidth)
            } else {
                None
            },
        }
    }

    fn generate_mount_config(&self, profile: &SandboxProfile) -> Vec<MountEntry> {
        let mut mounts = Vec::new();

        // Private tmp
        if profile.filesystem.private_tmp {
            mounts.push(MountEntry {
                source: "tmpfs".into(),
                target: "/tmp".into(),
                fstype: "tmpfs".into(),
                flags: MountFlags::empty(),
                options: "size=100M,mode=1777".into(),
            });
        }

        // Private dev
        if profile.filesystem.private_dev {
            mounts.push(MountEntry {
                source: "tmpfs".into(),
                target: "/dev".into(),
                fstype: "tmpfs".into(),
                flags: MountFlags::empty(),
                options: "size=64K,mode=755".into(),
            });
            // Essential devices
            for dev in &["null", "zero", "random", "urandom", "tty"] {
                mounts.push(MountEntry {
                    source: format!("/dev/{}", dev),
                    target: format!("/dev/{}", dev),
                    fstype: "none".into(),
                    flags: MountFlags::BIND,
                    options: String::new(),
                });
            }
        }

        // Read-only mounts
        for path in &profile.filesystem.read_only {
            mounts.push(MountEntry {
                source: path.display().to_string(),
                target: path.display().to_string(),
                fstype: "none".into(),
                flags: MountFlags::BIND | MountFlags::RDONLY,
                options: String::new(),
            });
        }

        // Read-write mounts
        for path in &profile.filesystem.read_write {
            mounts.push(MountEntry {
                source: path.display().to_string(),
                target: path.display().to_string(),
                fstype: "none".into(),
                flags: MountFlags::BIND,
                options: String::new(),
            });
        }

        mounts
    }

    fn compute_capabilities(&self, profile: &SandboxProfile) -> Vec<String> {
        let mut caps: HashSet<String> = profile.allowed_capabilities.iter().cloned().collect();

        // Remove explicitly denied
        for denied in &profile.denied_capabilities {
            caps.remove(denied);
        }

        caps.into_iter().collect()
    }
}

/// Kernel sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub namespaces: NamespaceConfig,
    pub seccomp: SeccompConfig,
    pub cgroups: CgroupConfig,
    pub mounts: Vec<MountEntry>,
    pub capabilities: Vec<String>,
}

/// Namespace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    pub user: bool,
    pub pid: bool,
    pub net: bool,
    pub ipc: bool,
    pub mount: bool,
    pub uts: bool,
    pub cgroup: bool,
}

/// Seccomp configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompConfig {
    pub default_action: SeccompAction,
    pub allowed_syscalls: Vec<String>,
    pub traced_syscalls: Vec<String>,
}

/// Seccomp action
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SeccompAction {
    Allow,
    Errno(i32),
    Trace,
    Kill,
}

/// Cgroup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    pub cpu_quota: Option<u32>,
    pub memory_max: Option<u64>,
    pub pids_max: Option<u32>,
    pub io_max: Option<u64>,
}

/// Mount entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountEntry {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub flags: MountFlags,
    pub options: String,
}

bitflags::bitflags! {
    /// Mount flags
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct MountFlags: u32 {
        const BIND = 0x1;
        const RDONLY = 0x2;
        const NOSUID = 0x4;
        const NODEV = 0x8;
        const NOEXEC = 0x10;
        const PRIVATE = 0x20;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_builder() {
        let profile = SandboxBuilder::new("test")
            .allow_path("/home/user/data", true)
            .allow_path("/usr/share", false)
            .allow_host("api.example.com")
            .allow_port(8080)
            .memory_limit(1024 * 1024 * 1024)
            .cpu_limit(50)
            .build();

        assert_eq!(profile.name, "test");
        assert!(profile.filesystem.read_write.contains(&PathBuf::from("/home/user/data")));
        assert!(profile.filesystem.read_only.contains(&PathBuf::from("/usr/share")));
        assert!(profile.network.allowed_hosts.contains(&"api.example.com".to_string()));
        assert!(profile.network.allowed_ports.contains(&8080));
        assert_eq!(profile.resources.memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(profile.resources.cpu_percent, 50);
    }

    #[test]
    fn test_sandbox_levels() {
        let enforcer = SandboxEnforcer::new();

        // Light should allow more
        let light = enforcer.get_profile(SandboxLevel::Light);
        assert!(light.network.allow_inbound);

        // Maximum should restrict heavily
        let max = enforcer.get_profile(SandboxLevel::Maximum);
        assert!(!max.network.enabled);
        assert!(!max.process.allow_spawn);
        assert!(max.process.user_namespace.is_some());
    }
}
