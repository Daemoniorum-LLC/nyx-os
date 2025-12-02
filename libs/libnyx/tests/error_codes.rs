//! Error code validation tests
//!
//! These tests verify that error codes in libnyx match the kernel's
//! SyscallError enum values. This ensures error handling works correctly.

use std::fs;
use std::collections::HashMap;

/// Parse error codes from libnyx syscall.rs
fn parse_libnyx_errors() -> HashMap<String, i32> {
    let content = fs::read_to_string("src/syscall.rs")
        .expect("Failed to read src/syscall.rs");

    let mut errors = HashMap::new();
    let mut in_enum = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.contains("pub enum Error") {
            in_enum = true;
            continue;
        }

        if in_enum {
            if trimmed == "}" {
                break;
            }

            // Parse lines like "InvalidSyscall = -1,"
            if let Some((name, value)) = parse_error_variant(trimmed) {
                errors.insert(name, value);
            }
        }
    }

    errors
}

fn parse_error_variant(line: &str) -> Option<(String, i32)> {
    // Skip comments and doc comments
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

    // Handle negative values
    let value: i32 = value_str.parse().ok()?;

    Some((name, value))
}

#[test]
fn test_error_codes_match_kernel() {
    let errors = parse_libnyx_errors();

    // Expected error codes from kernel/src/syscall.rs
    let expected: HashMap<&str, i32> = [
        ("Success", 0),
        ("InvalidSyscall", -1),
        ("InvalidCapability", -2),
        ("PermissionDenied", -3),
        ("OutOfMemory", -4),
        ("InvalidArgument", -5),
        ("WouldBlock", -6),
        ("Timeout", -7),
        ("Interrupted", -8),
        ("NotFound", -9),
        ("InvalidFormat", -10),
        ("IoError", -11),
        ("TooManyProcesses", -12),
        ("NoChild", -13),
        ("BadAddress", -14),
    ]
    .into_iter()
    .collect();

    for (name, &expected_value) in &expected {
        let actual = errors.get(*name);
        assert_eq!(
            actual,
            Some(&expected_value),
            "Error code mismatch for {}: expected {}, got {:?}",
            name,
            expected_value,
            actual
        );
    }
}

#[test]
fn test_error_codes_are_negative() {
    let errors = parse_libnyx_errors();

    for (name, &value) in &errors {
        if name != "Success" {
            assert!(
                value < 0,
                "Error {} should be negative, got {}",
                name,
                value
            );
        }
    }
}

#[test]
fn test_error_codes_are_unique() {
    let errors = parse_libnyx_errors();

    let mut seen_values: HashMap<i32, String> = HashMap::new();

    for (name, &value) in &errors {
        if let Some(existing) = seen_values.get(&value) {
            panic!(
                "Duplicate error code {}: {} and {}",
                value, existing, name
            );
        }
        seen_values.insert(value, name.clone());
    }
}

#[test]
fn test_error_codes_in_valid_range() {
    let errors = parse_libnyx_errors();

    for (name, &value) in &errors {
        assert!(
            value >= -128 && value <= 0,
            "Error code {} = {} is outside valid range [-128, 0]",
            name,
            value
        );
    }
}
