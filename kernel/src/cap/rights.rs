//! Capability rights definitions

use bitflags::bitflags;

bitflags! {
    /// Rights that can be granted via capabilities
    ///
    /// These are organized into categories:
    /// - Universal (0-7): Apply to all object types
    /// - Memory (8-15): Memory region specific
    /// - IPC (16-23): IPC endpoint specific
    /// - Process (24-31): Process/thread specific
    /// - Hardware (32-39): Hardware access
    /// - AI/Tensor (40-47): AI acceleration specific
    /// - Reserved (48-63): Future use
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Rights: u64 {
        // === Universal Rights (bits 0-7) ===

        /// Read data/state from object
        const READ = 1 << 0;
        /// Write/modify data/state
        const WRITE = 1 << 1;
        /// Execute code or invoke operations
        const EXECUTE = 1 << 2;
        /// Derive new capabilities (delegation)
        const GRANT = 1 << 3;
        /// Revoke derived capabilities
        const REVOKE = 1 << 4;
        /// Duplicate the capability itself
        const DUPLICATE = 1 << 5;
        /// Transfer capability to another CSpace
        const TRANSFER = 1 << 6;
        /// Inspect object metadata
        const INSPECT = 1 << 7;

        // === Memory Rights (bits 8-15) ===

        /// Map memory region into address space
        const MAP = 1 << 8;
        /// Unmap memory region
        const UNMAP = 1 << 9;
        /// Access as device memory (uncached)
        const DEVICE_MEM = 1 << 10;
        /// Lock memory (prevent swapping)
        const LOCK = 1 << 11;
        /// Share memory with other processes
        const SHARE = 1 << 12;
        /// Use huge pages
        const HUGE_PAGES = 1 << 13;
        /// Memory is persistent (survives power loss)
        const PERSISTENT = 1 << 14;

        // === IPC Rights (bits 16-23) ===

        /// Send messages to endpoint
        const SEND = 1 << 16;
        /// Receive messages from endpoint
        const RECEIVE = 1 << 17;
        /// Perform synchronous call (send + wait)
        const CALL = 1 << 18;
        /// Reply to a call
        const REPLY = 1 << 19;
        /// Signal notification bits
        const SIGNAL = 1 << 20;
        /// Wait on notification bits
        const WAIT = 1 << 21;
        /// Poll without blocking
        const POLL = 1 << 22;

        // === Process Rights (bits 24-31) ===

        /// Fork/spawn child processes
        const FORK = 1 << 24;
        /// Terminate process/thread
        const KILL = 1 << 25;
        /// Debug/trace execution
        const TRACE = 1 << 26;
        /// Record execution (time-travel)
        const RECORD = 1 << 27;
        /// Suspend execution
        const SUSPEND = 1 << 28;
        /// Resume execution
        const RESUME = 1 << 29;
        /// Modify scheduling parameters
        const SCHEDULE = 1 << 30;

        // === Hardware Rights (bits 32-39) ===

        /// Handle interrupts
        const IRQ = 1 << 32;
        /// Perform DMA operations
        const DMA = 1 << 33;
        /// Access memory-mapped I/O
        const MMIO = 1 << 34;
        /// Access I/O ports (x86)
        const IOPORT = 1 << 35;
        /// Access GPU resources
        const GPU = 1 << 36;
        /// Access NPU resources
        const NPU = 1 << 37;
        /// Access sensors (camera, mic, etc.)
        const SENSOR = 1 << 38;

        // === AI/Tensor Rights (bits 40-47) ===

        /// Allocate tensor buffers
        const TENSOR_ALLOC = 1 << 40;
        /// Free tensor buffers
        const TENSOR_FREE = 1 << 41;
        /// Submit inference requests
        const INFERENCE = 1 << 42;
        /// Submit GPU compute jobs
        const GPU_COMPUTE = 1 << 43;
        /// Access NPU for inference
        const NPU_ACCESS = 1 << 44;
        /// Migrate tensors between devices
        const TENSOR_MIGRATE = 1 << 45;
        /// Access model weights
        const MODEL_ACCESS = 1 << 46;

        // === Common Combinations ===

        /// Full memory access
        const MEMORY_FULL = Self::READ.bits() | Self::WRITE.bits() |
                           Self::MAP.bits() | Self::UNMAP.bits() |
                           Self::SHARE.bits() | Self::GRANT.bits();

        /// Read-only memory access
        const MEMORY_READ = Self::READ.bits() | Self::MAP.bits();

        /// Full IPC access
        const IPC_FULL = Self::SEND.bits() | Self::RECEIVE.bits() |
                        Self::CALL.bits() | Self::REPLY.bits() |
                        Self::SIGNAL.bits() | Self::WAIT.bits() |
                        Self::GRANT.bits();

        /// Client-side IPC (can call services)
        const IPC_CLIENT = Self::SEND.bits() | Self::CALL.bits() | Self::WAIT.bits();

        /// Server-side IPC (can receive and reply)
        const IPC_SERVER = Self::RECEIVE.bits() | Self::REPLY.bits() | Self::WAIT.bits();

        /// Full process control
        const PROCESS_FULL = Self::FORK.bits() | Self::KILL.bits() |
                            Self::TRACE.bits() | Self::SUSPEND.bits() |
                            Self::RESUME.bits() | Self::SCHEDULE.bits() |
                            Self::GRANT.bits();

        /// Full AI/inference access
        const AI_FULL = Self::TENSOR_ALLOC.bits() | Self::TENSOR_FREE.bits() |
                       Self::INFERENCE.bits() | Self::GPU_COMPUTE.bits() |
                       Self::NPU_ACCESS.bits() | Self::TENSOR_MIGRATE.bits() |
                       Self::MODEL_ACCESS.bits() | Self::GRANT.bits();

        /// Inference-only access (no model modification)
        const AI_INFERENCE = Self::TENSOR_ALLOC.bits() | Self::TENSOR_FREE.bits() |
                            Self::INFERENCE.bits() | Self::TENSOR_MIGRATE.bits();
    }
}

impl Rights {
    /// Check if this rights set is a subset of another
    #[inline]
    pub fn is_subset_of(self, other: Rights) -> bool {
        (self.bits() & !other.bits()) == 0
    }

    /// Get human-readable description of rights
    pub fn description(&self) -> alloc::string::String {
        use alloc::string::String;
        use alloc::vec::Vec;

        let mut parts: Vec<&str> = Vec::new();

        if self.contains(Rights::READ) {
            parts.push("read");
        }
        if self.contains(Rights::WRITE) {
            parts.push("write");
        }
        if self.contains(Rights::EXECUTE) {
            parts.push("exec");
        }
        if self.contains(Rights::GRANT) {
            parts.push("grant");
        }
        if self.contains(Rights::INFERENCE) {
            parts.push("inference");
        }
        if self.contains(Rights::GPU_COMPUTE) {
            parts.push("gpu");
        }
        // ... more as needed

        if parts.is_empty() {
            String::from("none")
        } else {
            parts.join("+")
        }
    }
}

impl core::fmt::Display for Rights {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subset() {
        let full = Rights::READ | Rights::WRITE | Rights::GRANT;
        let partial = Rights::READ;

        assert!(partial.is_subset_of(full));
        assert!(!full.is_subset_of(partial));
    }

    #[test]
    fn test_empty_rights() {
        let empty = Rights::empty();
        assert!(empty.is_empty());
        assert!(!empty.contains(Rights::READ));
        assert!(empty.is_subset_of(Rights::READ));
    }

    #[test]
    fn test_individual_rights() {
        assert_eq!(Rights::READ.bits(), 1 << 0);
        assert_eq!(Rights::WRITE.bits(), 1 << 1);
        assert_eq!(Rights::EXECUTE.bits(), 1 << 2);
        assert_eq!(Rights::GRANT.bits(), 1 << 3);
    }

    #[test]
    fn test_rights_or() {
        let rw = Rights::READ | Rights::WRITE;
        assert!(rw.contains(Rights::READ));
        assert!(rw.contains(Rights::WRITE));
        assert!(!rw.contains(Rights::EXECUTE));
    }

    #[test]
    fn test_rights_and() {
        let rwe = Rights::READ | Rights::WRITE | Rights::EXECUTE;
        let rw = Rights::READ | Rights::WRITE;
        let intersection = rwe & rw;
        assert!(intersection.contains(Rights::READ));
        assert!(intersection.contains(Rights::WRITE));
        assert!(!intersection.contains(Rights::EXECUTE));
    }

    #[test]
    fn test_rights_remove() {
        let mut rights = Rights::READ | Rights::WRITE | Rights::GRANT;
        rights.remove(Rights::WRITE);
        assert!(rights.contains(Rights::READ));
        assert!(!rights.contains(Rights::WRITE));
        assert!(rights.contains(Rights::GRANT));
    }

    #[test]
    fn test_common_combinations() {
        // AI_INFERENCE should be subset of AI_FULL
        assert!(Rights::AI_INFERENCE.is_subset_of(Rights::AI_FULL));

        // IPC_CLIENT should not be subset of IPC_SERVER
        assert!(!Rights::IPC_CLIENT.is_subset_of(Rights::IPC_SERVER));
    }

    #[test]
    fn test_memory_combinations() {
        // MEMORY_READ should be subset of MEMORY_FULL
        assert!(Rights::MEMORY_READ.is_subset_of(Rights::MEMORY_FULL));

        // MEMORY_FULL should contain all expected rights
        assert!(Rights::MEMORY_FULL.contains(Rights::READ));
        assert!(Rights::MEMORY_FULL.contains(Rights::WRITE));
        assert!(Rights::MEMORY_FULL.contains(Rights::MAP));
        assert!(Rights::MEMORY_FULL.contains(Rights::UNMAP));
    }

    #[test]
    fn test_ipc_combinations() {
        // IPC_FULL contains all IPC rights
        assert!(Rights::IPC_FULL.contains(Rights::SEND));
        assert!(Rights::IPC_FULL.contains(Rights::RECEIVE));
        assert!(Rights::IPC_FULL.contains(Rights::CALL));
        assert!(Rights::IPC_FULL.contains(Rights::REPLY));

        // IPC_CLIENT contains SEND, CALL, WAIT
        assert!(Rights::IPC_CLIENT.contains(Rights::SEND));
        assert!(Rights::IPC_CLIENT.contains(Rights::CALL));
        assert!(Rights::IPC_CLIENT.contains(Rights::WAIT));
        assert!(!Rights::IPC_CLIENT.contains(Rights::RECEIVE));
    }

    #[test]
    fn test_process_combinations() {
        assert!(Rights::PROCESS_FULL.contains(Rights::FORK));
        assert!(Rights::PROCESS_FULL.contains(Rights::KILL));
        assert!(Rights::PROCESS_FULL.contains(Rights::SUSPEND));
        assert!(Rights::PROCESS_FULL.contains(Rights::RESUME));
    }

    #[test]
    fn test_description() {
        let read_only = Rights::READ;
        assert_eq!(read_only.description(), "read");

        let rw = Rights::READ | Rights::WRITE;
        let desc = rw.description();
        assert!(desc.contains("read"));
        assert!(desc.contains("write"));

        let empty = Rights::empty();
        assert_eq!(empty.description(), "none");
    }

    #[test]
    fn test_hardware_rights() {
        assert_eq!(Rights::IRQ.bits(), 1 << 32);
        assert_eq!(Rights::DMA.bits(), 1 << 33);
        assert_eq!(Rights::MMIO.bits(), 1 << 34);
    }

    #[test]
    fn test_is_subset_reflexive() {
        let rights = Rights::READ | Rights::WRITE;
        assert!(rights.is_subset_of(rights));
    }

    #[test]
    fn test_all_rights_independent() {
        // Each right should not overlap with others (non-combination rights)
        let individual_rights = [
            Rights::READ,
            Rights::WRITE,
            Rights::EXECUTE,
            Rights::GRANT,
            Rights::REVOKE,
            Rights::DUPLICATE,
            Rights::TRANSFER,
            Rights::INSPECT,
            Rights::MAP,
            Rights::UNMAP,
            Rights::SEND,
            Rights::RECEIVE,
        ];

        for (i, &a) in individual_rights.iter().enumerate() {
            for (j, &b) in individual_rights.iter().enumerate() {
                if i != j {
                    assert!(!a.contains(b), "{:?} should not contain {:?}", a, b);
                }
            }
        }
    }
}
