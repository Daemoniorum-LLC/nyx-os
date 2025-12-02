//! Build script for libnyx
//!
//! Validates that syscall numbers in libnyx match the kernel definitions.
//! This prevents the silent failures that occur when userspace and kernel
//! syscall numbers drift apart.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/syscall.rs");
    println!("cargo:rerun-if-changed=../../kernel/src/syscall.rs");

    // Only run validation if kernel source is available
    let kernel_syscall_path = Path::new("../../kernel/src/syscall.rs");
    if !kernel_syscall_path.exists() {
        println!("cargo:warning=Kernel source not found, skipping syscall validation");
        return;
    }

    let kernel_syscalls = parse_kernel_syscalls(kernel_syscall_path);
    let libnyx_syscalls = parse_libnyx_syscalls(Path::new("src/syscall.rs"));

    validate_syscalls(&kernel_syscalls, &libnyx_syscalls);
}

/// Parse syscall enum from kernel/src/syscall.rs
fn parse_kernel_syscalls(path: &Path) -> HashMap<String, u64> {
    let content = fs::read_to_string(path).expect("Failed to read kernel syscall.rs");
    let mut syscalls = HashMap::new();

    // Parse enum Syscall { ... }
    let mut in_enum = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("pub enum Syscall") {
            in_enum = true;
            continue;
        }

        if in_enum {
            if trimmed == "}" {
                break;
            }

            // Parse lines like "RingSetup = 0,"
            if let Some((name, value)) = parse_enum_variant(trimmed) {
                syscalls.insert(name, value);
            }
        }
    }

    syscalls
}

/// Parse syscall constants from libnyx/src/syscall.rs
fn parse_libnyx_syscalls(path: &Path) -> HashMap<String, u64> {
    let content = fs::read_to_string(path).expect("Failed to read libnyx syscall.rs");
    let mut syscalls = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Parse lines like "pub const RING_SETUP: u64 = 0;"
        if trimmed.starts_with("pub const") && trimmed.contains(": u64 =") {
            if let Some((name, value)) = parse_const_definition(trimmed) {
                syscalls.insert(name, value);
            }
        }
    }

    syscalls
}

/// Parse an enum variant line like "RingSetup = 0,"
fn parse_enum_variant(line: &str) -> Option<(String, u64)> {
    // Skip comments and empty lines
    if line.starts_with("//") || line.is_empty() {
        return None;
    }

    // Find "Name = value"
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() != 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let value_str = parts[1].trim().trim_end_matches(',');

    // Skip non-numeric (like comments after value)
    let value_str = value_str.split_whitespace().next()?;
    let value: u64 = value_str.parse().ok()?;

    Some((name, value))
}

/// Parse a const definition like "pub const RING_SETUP: u64 = 0;"
fn parse_const_definition(line: &str) -> Option<(String, u64)> {
    // Extract: pub const NAME: u64 = VALUE;
    let line = line.strip_prefix("pub const ")?;
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].trim().to_string();

    // Get the value after "u64 ="
    let rest = parts[1..].join(":");
    let value_part = rest.split('=').nth(1)?;
    let value_str = value_part.trim().trim_end_matches(';');
    let value: u64 = value_str.parse().ok()?;

    Some((name, value))
}

/// Validate that libnyx syscalls match kernel syscalls
fn validate_syscalls(kernel: &HashMap<String, u64>, libnyx: &HashMap<String, u64>) {
    let mut errors = Vec::new();

    // Map kernel enum names to libnyx const names
    let name_mapping: HashMap<&str, &str> = [
        ("RingSetup", "RING_SETUP"),
        ("RingEnter", "RING_ENTER"),
        ("Send", "SEND"),
        ("Receive", "RECEIVE"),
        ("Call", "CALL"),
        ("Reply", "REPLY"),
        ("Signal", "SIGNAL"),
        ("Wait", "WAIT"),
        ("Poll", "POLL"),
        ("CapDerive", "CAP_DERIVE"),
        ("CapRevoke", "CAP_REVOKE"),
        ("CapIdentify", "CAP_IDENTIFY"),
        ("CapGrant", "CAP_GRANT"),
        ("CapDrop", "CAP_DROP"),
        ("MemMap", "MEM_MAP"),
        ("MemUnmap", "MEM_UNMAP"),
        ("MemProtect", "MEM_PROTECT"),
        ("MemAlloc", "MEM_ALLOC"),
        ("MemFree", "MEM_FREE"),
        ("ThreadCreate", "THREAD_CREATE"),
        ("ThreadExit", "THREAD_EXIT"),
        ("ThreadYield", "THREAD_YIELD"),
        ("ThreadSleep", "THREAD_SLEEP"),
        ("ThreadJoin", "THREAD_JOIN"),
        ("ProcessSpawn", "PROCESS_SPAWN"),
        ("ProcessExit", "PROCESS_EXIT"),
        ("ProcessWait", "PROCESS_WAIT"),
        ("ProcessGetPid", "PROCESS_GETPID"),
        ("ProcessGetPpid", "PROCESS_GETPPID"),
        ("FsOpen", "FS_OPEN"),
        ("FsClose", "FS_CLOSE"),
        ("FsRead", "FS_READ"),
        ("FsWrite", "FS_WRITE"),
        ("FsStat", "FS_STAT"),
        ("FsReaddir", "FS_READDIR"),
        ("TensorAlloc", "TENSOR_ALLOC"),
        ("TensorFree", "TENSOR_FREE"),
        ("TensorMigrate", "TENSOR_MIGRATE"),
        ("InferenceCreate", "INFERENCE_CREATE"),
        ("InferenceSubmit", "INFERENCE_SUBMIT"),
        ("ComputeSubmit", "COMPUTE_SUBMIT"),
        ("Checkpoint", "CHECKPOINT"),
        ("Restore", "RESTORE"),
        ("RecordStart", "RECORD_START"),
        ("RecordStop", "RECORD_STOP"),
        ("Debug", "DEBUG"),
        ("GetTime", "GET_TIME"),
        ("Reboot", "REBOOT"),
        ("Shutdown", "SHUTDOWN"),
    ]
    .into_iter()
    .collect();

    for (kernel_name, &libnyx_name) in &name_mapping {
        let kernel_value = kernel.get(*kernel_name);
        let libnyx_value = libnyx.get(libnyx_name);

        match (kernel_value, libnyx_value) {
            (Some(&kv), Some(&lv)) => {
                if kv != lv {
                    errors.push(format!(
                        "MISMATCH: {} (kernel={}, libnyx {}={})",
                        kernel_name, kv, libnyx_name, lv
                    ));
                }
            }
            (Some(&kv), None) => {
                errors.push(format!(
                    "MISSING in libnyx: {} = {} (expected const {})",
                    kernel_name, kv, libnyx_name
                ));
            }
            (None, Some(&lv)) => {
                // This is okay - libnyx might have extra definitions
                println!(
                    "cargo:warning=Extra syscall in libnyx: {} = {} (not in kernel enum)",
                    libnyx_name, lv
                );
            }
            (None, None) => {
                // Both missing - nothing to compare
            }
        }
    }

    if !errors.is_empty() {
        println!("cargo:warning===========================================");
        println!("cargo:warning=SYSCALL NUMBER MISMATCH DETECTED!");
        println!("cargo:warning===========================================");
        for error in &errors {
            println!("cargo:warning={}", error);
        }
        println!("cargo:warning===========================================");

        // In release mode, fail the build
        #[cfg(not(debug_assertions))]
        {
            panic!(
                "Syscall number mismatch between kernel and libnyx! {} errors found.",
                errors.len()
            );
        }
    } else {
        println!("cargo:warning=Syscall validation passed: all numbers match kernel");
    }
}
