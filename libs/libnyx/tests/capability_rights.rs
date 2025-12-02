//! Capability rights validation tests
//!
//! These tests verify that capability rights in libnyx match the kernel's
//! Rights bitflags. This is critical for security - mismatched rights would
//! cause access control failures.

use std::fs;
use std::collections::HashMap;

/// Parse rights constants from a source file
fn parse_rights(content: &str) -> HashMap<String, u64> {
    let mut rights = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Parse lines like "const READ = 1 << 0;"
        if trimmed.starts_with("const ") && trimmed.contains(" = 1 << ") {
            if let Some((name, bit)) = parse_rights_constant(trimmed) {
                rights.insert(name, 1u64 << bit);
            }
        }
    }

    rights
}

fn parse_rights_constant(line: &str) -> Option<(String, u32)> {
    // Parse: const NAME = 1 << N;
    let line = line.strip_prefix("const ")?;
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() != 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let value_part = parts[1].trim().trim_end_matches(';');

    // Parse "1 << N"
    if let Some(bit_str) = value_part.strip_prefix("1 << ") {
        let bit: u32 = bit_str.trim().parse().ok()?;
        return Some((name, bit));
    }

    None
}

#[test]
fn test_universal_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // Universal rights (bits 0-7)
    assert_eq!(rights.get("READ"), Some(&(1 << 0)), "READ should be bit 0");
    assert_eq!(rights.get("WRITE"), Some(&(1 << 1)), "WRITE should be bit 1");
    assert_eq!(rights.get("EXECUTE"), Some(&(1 << 2)), "EXECUTE should be bit 2");
    assert_eq!(rights.get("GRANT"), Some(&(1 << 3)), "GRANT should be bit 3");
    assert_eq!(rights.get("REVOKE"), Some(&(1 << 4)), "REVOKE should be bit 4");
    assert_eq!(rights.get("DUPLICATE"), Some(&(1 << 5)), "DUPLICATE should be bit 5");
    assert_eq!(rights.get("TRANSFER"), Some(&(1 << 6)), "TRANSFER should be bit 6");
    assert_eq!(rights.get("INSPECT"), Some(&(1 << 7)), "INSPECT should be bit 7");
}

#[test]
fn test_memory_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // Memory rights (bits 8-15)
    assert_eq!(rights.get("MAP"), Some(&(1 << 8)), "MAP should be bit 8");
    assert_eq!(rights.get("UNMAP"), Some(&(1 << 9)), "UNMAP should be bit 9");
    assert_eq!(rights.get("DEVICE_MEM"), Some(&(1 << 10)), "DEVICE_MEM should be bit 10");
    assert_eq!(rights.get("LOCK"), Some(&(1 << 11)), "LOCK should be bit 11");
    assert_eq!(rights.get("SHARE"), Some(&(1 << 12)), "SHARE should be bit 12");
    assert_eq!(rights.get("HUGE_PAGES"), Some(&(1 << 13)), "HUGE_PAGES should be bit 13");
    assert_eq!(rights.get("PERSISTENT"), Some(&(1 << 14)), "PERSISTENT should be bit 14");
}

#[test]
fn test_ipc_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // IPC rights (bits 16-23)
    assert_eq!(rights.get("SEND"), Some(&(1 << 16)), "SEND should be bit 16");
    assert_eq!(rights.get("RECEIVE"), Some(&(1 << 17)), "RECEIVE should be bit 17");
    assert_eq!(rights.get("CALL"), Some(&(1 << 18)), "CALL should be bit 18");
    assert_eq!(rights.get("REPLY"), Some(&(1 << 19)), "REPLY should be bit 19");
    assert_eq!(rights.get("SIGNAL"), Some(&(1 << 20)), "SIGNAL should be bit 20");
    assert_eq!(rights.get("WAIT"), Some(&(1 << 21)), "WAIT should be bit 21");
    assert_eq!(rights.get("POLL"), Some(&(1 << 22)), "POLL should be bit 22");
}

#[test]
fn test_process_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // Process rights (bits 24-31)
    assert_eq!(rights.get("FORK"), Some(&(1 << 24)), "FORK should be bit 24");
    assert_eq!(rights.get("KILL"), Some(&(1 << 25)), "KILL should be bit 25");
    assert_eq!(rights.get("TRACE"), Some(&(1 << 26)), "TRACE should be bit 26");
    assert_eq!(rights.get("RECORD"), Some(&(1 << 27)), "RECORD should be bit 27");
    assert_eq!(rights.get("SUSPEND"), Some(&(1 << 28)), "SUSPEND should be bit 28");
    assert_eq!(rights.get("RESUME"), Some(&(1 << 29)), "RESUME should be bit 29");
    assert_eq!(rights.get("SCHEDULE"), Some(&(1 << 30)), "SCHEDULE should be bit 30");
}

#[test]
fn test_hardware_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // Hardware rights (bits 32-39)
    assert_eq!(rights.get("IRQ"), Some(&(1 << 32)), "IRQ should be bit 32");
    assert_eq!(rights.get("DMA"), Some(&(1 << 33)), "DMA should be bit 33");
    assert_eq!(rights.get("MMIO"), Some(&(1 << 34)), "MMIO should be bit 34");
    assert_eq!(rights.get("IOPORT"), Some(&(1 << 35)), "IOPORT should be bit 35");
    assert_eq!(rights.get("GPU"), Some(&(1 << 36)), "GPU should be bit 36");
    assert_eq!(rights.get("NPU"), Some(&(1 << 37)), "NPU should be bit 37");
    assert_eq!(rights.get("SENSOR"), Some(&(1 << 38)), "SENSOR should be bit 38");
}

#[test]
fn test_ai_tensor_rights() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // AI/Tensor rights (bits 40-47)
    assert_eq!(rights.get("TENSOR_ALLOC"), Some(&(1 << 40)), "TENSOR_ALLOC should be bit 40");
    assert_eq!(rights.get("TENSOR_FREE"), Some(&(1 << 41)), "TENSOR_FREE should be bit 41");
    assert_eq!(rights.get("INFERENCE"), Some(&(1 << 42)), "INFERENCE should be bit 42");
    assert_eq!(rights.get("GPU_COMPUTE"), Some(&(1 << 43)), "GPU_COMPUTE should be bit 43");
    assert_eq!(rights.get("NPU_ACCESS"), Some(&(1 << 44)), "NPU_ACCESS should be bit 44");
    assert_eq!(rights.get("TENSOR_MIGRATE"), Some(&(1 << 45)), "TENSOR_MIGRATE should be bit 45");
    assert_eq!(rights.get("MODEL_ACCESS"), Some(&(1 << 46)), "MODEL_ACCESS should be bit 46");
}

#[test]
fn test_rights_do_not_overlap() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    // Check that no two rights have the same value
    let mut seen_values: HashMap<u64, String> = HashMap::new();

    for (name, &value) in &rights {
        // Skip combination rights
        if name.contains("FULL") || name.contains("CLIENT") ||
           name.contains("SERVER") || name.contains("INFERENCE") && name != "INFERENCE" {
            continue;
        }

        if let Some(existing) = seen_values.get(&value) {
            panic!(
                "Rights {} and {} have the same value {}",
                existing, name, value
            );
        }
        seen_values.insert(value, name.clone());
    }
}

#[test]
fn test_rights_are_powers_of_two() {
    let content = fs::read_to_string("src/cap.rs")
        .expect("Failed to read src/cap.rs");
    let rights = parse_rights(&content);

    for (name, &value) in &rights {
        // Skip combination rights
        if name.contains("FULL") || name.contains("CLIENT") ||
           name.contains("SERVER") || name.contains("_READ") ||
           (name.contains("INFERENCE") && name != "INFERENCE") {
            continue;
        }

        assert!(
            value.is_power_of_two(),
            "Right {} = {} is not a power of two",
            name,
            value
        );
    }
}
