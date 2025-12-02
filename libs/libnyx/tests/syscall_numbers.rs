//! Syscall number validation tests
//!
//! These tests verify that the syscall numbers in libnyx match the expected
//! kernel ABI. This catches any drift between kernel and userspace.

// Note: These tests run on the host, not in the Nyx kernel.
// They validate the constants are correctly defined.

mod syscall_numbers {
    // We can't directly use libnyx (it's no_std), so we redefine the expected values
    // and verify they match what's documented in the kernel.

    // Expected syscall numbers from kernel/src/syscall.rs
    mod expected {
        // IPC (0-15)
        pub const RING_SETUP: u64 = 0;
        pub const RING_ENTER: u64 = 1;
        pub const SEND: u64 = 2;
        pub const RECEIVE: u64 = 3;
        pub const CALL: u64 = 4;
        pub const REPLY: u64 = 5;
        pub const SIGNAL: u64 = 6;
        pub const WAIT: u64 = 7;
        pub const POLL: u64 = 8;

        // Capabilities (16-31)
        pub const CAP_DERIVE: u64 = 16;
        pub const CAP_REVOKE: u64 = 17;
        pub const CAP_IDENTIFY: u64 = 18;
        pub const CAP_GRANT: u64 = 19;
        pub const CAP_DROP: u64 = 20;

        // Memory (32-63)
        pub const MEM_MAP: u64 = 32;
        pub const MEM_UNMAP: u64 = 33;
        pub const MEM_PROTECT: u64 = 34;
        pub const MEM_ALLOC: u64 = 35;
        pub const MEM_FREE: u64 = 36;

        // Threads (64-79)
        pub const THREAD_CREATE: u64 = 64;
        pub const THREAD_EXIT: u64 = 65;
        pub const THREAD_YIELD: u64 = 66;
        pub const THREAD_SLEEP: u64 = 67;
        pub const THREAD_JOIN: u64 = 68;

        // Process (80-95)
        pub const PROCESS_SPAWN: u64 = 80;
        pub const PROCESS_EXIT: u64 = 81;
        pub const PROCESS_WAIT: u64 = 82;
        pub const PROCESS_GETPID: u64 = 83;
        pub const PROCESS_GETPPID: u64 = 84;

        // Tensor/AI (112-143)
        pub const TENSOR_ALLOC: u64 = 112;
        pub const TENSOR_FREE: u64 = 113;
        pub const TENSOR_MIGRATE: u64 = 114;
        pub const INFERENCE_CREATE: u64 = 115;
        pub const INFERENCE_SUBMIT: u64 = 116;
        pub const COMPUTE_SUBMIT: u64 = 117;

        // Time-Travel (144-159)
        pub const CHECKPOINT: u64 = 144;
        pub const RESTORE: u64 = 145;
        pub const RECORD_START: u64 = 146;
        pub const RECORD_STOP: u64 = 147;

        // System (240-255)
        pub const DEBUG: u64 = 240;
        pub const GET_TIME: u64 = 241;
        pub const REBOOT: u64 = 254;
        pub const SHUTDOWN: u64 = 255;
    }

    /// Parse syscall constants from libnyx source
    fn parse_libnyx_syscalls() -> std::collections::HashMap<String, u64> {
        let content = std::fs::read_to_string("src/syscall.rs")
            .expect("Failed to read src/syscall.rs");

        let mut syscalls = std::collections::HashMap::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub const") && trimmed.contains(": u64 =") {
                // Parse: pub const NAME: u64 = VALUE;
                if let Some(name_start) = trimmed.strip_prefix("pub const ") {
                    let parts: Vec<&str> = name_start.split(':').collect();
                    if parts.len() >= 2 {
                        let name = parts[0].trim().to_string();
                        let rest = parts[1..].join(":");
                        if let Some(value_part) = rest.split('=').nth(1) {
                            let value_str = value_part.trim().trim_end_matches(';');
                            if let Ok(value) = value_str.parse::<u64>() {
                                syscalls.insert(name, value);
                            }
                        }
                    }
                }
            }
        }

        syscalls
    }

    #[test]
    fn test_ipc_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("RING_SETUP"), Some(&expected::RING_SETUP));
        assert_eq!(libnyx.get("RING_ENTER"), Some(&expected::RING_ENTER));
        assert_eq!(libnyx.get("SEND"), Some(&expected::SEND));
        assert_eq!(libnyx.get("RECEIVE"), Some(&expected::RECEIVE));
        assert_eq!(libnyx.get("CALL"), Some(&expected::CALL));
        assert_eq!(libnyx.get("REPLY"), Some(&expected::REPLY));
        assert_eq!(libnyx.get("SIGNAL"), Some(&expected::SIGNAL));
        assert_eq!(libnyx.get("WAIT"), Some(&expected::WAIT));
        assert_eq!(libnyx.get("POLL"), Some(&expected::POLL));
    }

    #[test]
    fn test_capability_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("CAP_DERIVE"), Some(&expected::CAP_DERIVE));
        assert_eq!(libnyx.get("CAP_REVOKE"), Some(&expected::CAP_REVOKE));
        assert_eq!(libnyx.get("CAP_IDENTIFY"), Some(&expected::CAP_IDENTIFY));
        assert_eq!(libnyx.get("CAP_GRANT"), Some(&expected::CAP_GRANT));
        assert_eq!(libnyx.get("CAP_DROP"), Some(&expected::CAP_DROP));
    }

    #[test]
    fn test_memory_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("MEM_MAP"), Some(&expected::MEM_MAP));
        assert_eq!(libnyx.get("MEM_UNMAP"), Some(&expected::MEM_UNMAP));
        assert_eq!(libnyx.get("MEM_PROTECT"), Some(&expected::MEM_PROTECT));
        assert_eq!(libnyx.get("MEM_ALLOC"), Some(&expected::MEM_ALLOC));
        assert_eq!(libnyx.get("MEM_FREE"), Some(&expected::MEM_FREE));
    }

    #[test]
    fn test_thread_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("THREAD_CREATE"), Some(&expected::THREAD_CREATE));
        assert_eq!(libnyx.get("THREAD_EXIT"), Some(&expected::THREAD_EXIT));
        assert_eq!(libnyx.get("THREAD_YIELD"), Some(&expected::THREAD_YIELD));
        assert_eq!(libnyx.get("THREAD_SLEEP"), Some(&expected::THREAD_SLEEP));
        assert_eq!(libnyx.get("THREAD_JOIN"), Some(&expected::THREAD_JOIN));
    }

    #[test]
    fn test_process_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("PROCESS_SPAWN"), Some(&expected::PROCESS_SPAWN));
        assert_eq!(libnyx.get("PROCESS_EXIT"), Some(&expected::PROCESS_EXIT));
        assert_eq!(libnyx.get("PROCESS_WAIT"), Some(&expected::PROCESS_WAIT));
        assert_eq!(libnyx.get("PROCESS_GETPID"), Some(&expected::PROCESS_GETPID));
        assert_eq!(libnyx.get("PROCESS_GETPPID"), Some(&expected::PROCESS_GETPPID));
    }

    #[test]
    fn test_tensor_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("TENSOR_ALLOC"), Some(&expected::TENSOR_ALLOC));
        assert_eq!(libnyx.get("TENSOR_FREE"), Some(&expected::TENSOR_FREE));
        assert_eq!(libnyx.get("TENSOR_MIGRATE"), Some(&expected::TENSOR_MIGRATE));
        assert_eq!(libnyx.get("INFERENCE_CREATE"), Some(&expected::INFERENCE_CREATE));
        assert_eq!(libnyx.get("INFERENCE_SUBMIT"), Some(&expected::INFERENCE_SUBMIT));
        assert_eq!(libnyx.get("COMPUTE_SUBMIT"), Some(&expected::COMPUTE_SUBMIT));
    }

    #[test]
    fn test_timetravel_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("CHECKPOINT"), Some(&expected::CHECKPOINT));
        assert_eq!(libnyx.get("RESTORE"), Some(&expected::RESTORE));
        assert_eq!(libnyx.get("RECORD_START"), Some(&expected::RECORD_START));
        assert_eq!(libnyx.get("RECORD_STOP"), Some(&expected::RECORD_STOP));
    }

    #[test]
    fn test_system_syscall_numbers() {
        let libnyx = parse_libnyx_syscalls();

        assert_eq!(libnyx.get("DEBUG"), Some(&expected::DEBUG));
        assert_eq!(libnyx.get("GET_TIME"), Some(&expected::GET_TIME));
        assert_eq!(libnyx.get("REBOOT"), Some(&expected::REBOOT));
        assert_eq!(libnyx.get("SHUTDOWN"), Some(&expected::SHUTDOWN));
    }

    #[test]
    fn test_syscall_ranges() {
        let libnyx = parse_libnyx_syscalls();

        // Verify syscalls are in correct ranges
        for (name, &value) in &libnyx {
            let expected_range = match name.as_str() {
                n if n.starts_with("RING_") || n == "SEND" || n == "RECEIVE" ||
                     n == "CALL" || n == "REPLY" || n == "SIGNAL" ||
                     n == "WAIT" || n == "POLL" => 0..16,
                n if n.starts_with("CAP_") => 16..32,
                n if n.starts_with("MEM_") => 32..64,
                n if n.starts_with("THREAD_") => 64..80,
                n if n.starts_with("PROCESS_") => 80..96,
                n if n.starts_with("FS_") => 96..112,
                n if n.starts_with("TENSOR_") || n.starts_with("INFERENCE_") ||
                     n == "COMPUTE_SUBMIT" => 112..144,
                n if n == "CHECKPOINT" || n == "RESTORE" ||
                     n.starts_with("RECORD_") => 144..160,
                n if n == "DEBUG" || n == "GET_TIME" || n == "REBOOT" ||
                     n == "SHUTDOWN" => 240..256,
                _ => continue,
            };

            assert!(
                expected_range.contains(&value),
                "Syscall {} = {} is not in expected range {:?}",
                name, value, expected_range
            );
        }
    }
}
