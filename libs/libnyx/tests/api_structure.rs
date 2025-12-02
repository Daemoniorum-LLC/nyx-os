//! API structure validation tests
//!
//! These tests verify that the libnyx API has all the expected types,
//! functions, and constants. This ensures the public API is complete.

use std::fs;

/// Parse a Rust source file and extract public items
fn extract_public_items(content: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut functions = Vec::new();
    let mut types = Vec::new();
    let mut constants = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle "pub fn" and "pub unsafe fn"
        if trimmed.starts_with("pub fn ") {
            if let Some(name) = extract_function_name(trimmed, "pub fn ") {
                functions.push(name);
            }
        } else if trimmed.starts_with("pub unsafe fn ") {
            if let Some(name) = extract_function_name(trimmed, "pub unsafe fn ") {
                functions.push(name);
            }
        } else if trimmed.starts_with("pub struct ") {
            if let Some(name) = extract_type_name(trimmed, "pub struct ") {
                types.push(name);
            }
        } else if trimmed.starts_with("pub enum ") {
            if let Some(name) = extract_type_name(trimmed, "pub enum ") {
                types.push(name);
            }
        } else if trimmed.starts_with("pub const ") {
            if let Some(name) = extract_const_name(trimmed) {
                constants.push(name);
            }
        }
    }

    (functions, types, constants)
}

fn extract_function_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    let name = rest.split(&['(', '<'][..]).next()?;
    Some(name.to_string())
}

fn extract_type_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    let name = rest.split(&[' ', '<', '{', '('][..]).next()?;
    Some(name.to_string())
}

fn extract_const_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("pub const ")?;
    let name = rest.split(':').next()?;
    Some(name.trim().to_string())
}

mod process_module {
    use super::*;

    #[test]
    fn test_process_types_exist() {
        let content = fs::read_to_string("src/process.rs")
            .expect("Failed to read src/process.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"ProcessId".to_string()), "Missing ProcessId type");
        assert!(types.contains(&"WaitResult".to_string()), "Missing WaitResult type");
    }

    #[test]
    fn test_process_functions_exist() {
        let content = fs::read_to_string("src/process.rs")
            .expect("Failed to read src/process.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"getpid".to_string()), "Missing getpid function");
        assert!(functions.contains(&"getppid".to_string()), "Missing getppid function");
        assert!(functions.contains(&"spawn".to_string()), "Missing spawn function");
        assert!(functions.contains(&"exit".to_string()), "Missing exit function");
        assert!(functions.contains(&"wait".to_string()), "Missing wait function");
    }
}

mod thread_module {
    use super::*;

    #[test]
    fn test_thread_types_exist() {
        let content = fs::read_to_string("src/thread.rs")
            .expect("Failed to read src/thread.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"ThreadId".to_string()), "Missing ThreadId type");
    }

    #[test]
    fn test_thread_functions_exist() {
        let content = fs::read_to_string("src/thread.rs")
            .expect("Failed to read src/thread.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"thread_create".to_string()), "Missing thread_create function");
        assert!(functions.contains(&"thread_exit".to_string()), "Missing thread_exit function");
        assert!(functions.contains(&"thread_yield".to_string()), "Missing thread_yield function");
        assert!(functions.contains(&"thread_sleep".to_string()), "Missing thread_sleep function");
        assert!(functions.contains(&"thread_join".to_string()), "Missing thread_join function");
        assert!(functions.contains(&"sleep_ms".to_string()), "Missing sleep_ms function");
        assert!(functions.contains(&"sleep_secs".to_string()), "Missing sleep_secs function");
    }
}

mod memory_module {
    use super::*;

    #[test]
    fn test_memory_functions_exist() {
        let content = fs::read_to_string("src/memory.rs")
            .expect("Failed to read src/memory.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"mmap".to_string()), "Missing mmap function");
        assert!(functions.contains(&"munmap".to_string()), "Missing munmap function");
        assert!(functions.contains(&"mprotect".to_string()), "Missing mprotect function");
        assert!(functions.contains(&"alloc".to_string()), "Missing alloc function");
        assert!(functions.contains(&"free".to_string()), "Missing free function");
        assert!(functions.contains(&"alloc_page".to_string()), "Missing alloc_page function");
        assert!(functions.contains(&"alloc_pages".to_string()), "Missing alloc_pages function");
    }

    #[test]
    fn test_memory_constants_exist() {
        let content = fs::read_to_string("src/memory.rs")
            .expect("Failed to read src/memory.rs");
        let (_, _, constants) = extract_public_items(&content);

        assert!(constants.contains(&"PAGE_SIZE".to_string()), "Missing PAGE_SIZE constant");
    }
}

mod ipc_module {
    use super::*;

    #[test]
    fn test_ipc_types_exist() {
        let content = fs::read_to_string("src/ipc.rs")
            .expect("Failed to read src/ipc.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"IpcRing".to_string()), "Missing IpcRing type");
        assert!(types.contains(&"Message".to_string()), "Missing Message type");
    }

    #[test]
    fn test_ipc_functions_exist() {
        let content = fs::read_to_string("src/ipc.rs")
            .expect("Failed to read src/ipc.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"send".to_string()), "Missing send function");
        assert!(functions.contains(&"receive".to_string()), "Missing receive function");
        assert!(functions.contains(&"call".to_string()), "Missing call function");
        assert!(functions.contains(&"reply".to_string()), "Missing reply function");
        assert!(functions.contains(&"signal".to_string()), "Missing signal function");
        assert!(functions.contains(&"wait".to_string()), "Missing wait function");
        assert!(functions.contains(&"poll".to_string()), "Missing poll function");
    }

    #[test]
    fn test_ipc_constants_exist() {
        let content = fs::read_to_string("src/ipc.rs")
            .expect("Failed to read src/ipc.rs");
        let (_, _, constants) = extract_public_items(&content);

        assert!(constants.contains(&"MAX_MESSAGE_SIZE".to_string()), "Missing MAX_MESSAGE_SIZE constant");
    }
}

mod cap_module {
    use super::*;

    #[test]
    fn test_cap_types_exist() {
        let content = fs::read_to_string("src/cap.rs")
            .expect("Failed to read src/cap.rs");
        let (_, types, _) = extract_public_items(&content);

        // Rights is defined via bitflags! macro, so check for its presence differently
        assert!(content.contains("pub struct Rights: u64"), "Missing Rights type (bitflags)");
        assert!(types.contains(&"Capability".to_string()), "Missing Capability type");
        assert!(types.contains(&"ObjectType".to_string()), "Missing ObjectType type");
    }
}

mod tensor_module {
    use super::*;

    #[test]
    fn test_tensor_types_exist() {
        let content = fs::read_to_string("src/tensor.rs")
            .expect("Failed to read src/tensor.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"Device".to_string()), "Missing Device type");
        assert!(types.contains(&"TensorShape".to_string()), "Missing TensorShape type");
        assert!(types.contains(&"DType".to_string()), "Missing DType type");
        assert!(types.contains(&"TensorBuffer".to_string()), "Missing TensorBuffer type");
        assert!(types.contains(&"InferenceConfig".to_string()), "Missing InferenceConfig type");
    }

    #[test]
    fn test_tensor_functions_exist() {
        let content = fs::read_to_string("src/tensor.rs")
            .expect("Failed to read src/tensor.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"inference_create".to_string()), "Missing inference_create function");
        assert!(functions.contains(&"inference_submit".to_string()), "Missing inference_submit function");
    }
}

mod time_module {
    use super::*;

    #[test]
    fn test_time_types_exist() {
        let content = fs::read_to_string("src/time.rs")
            .expect("Failed to read src/time.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"Instant".to_string()), "Missing Instant type");
    }

    #[test]
    fn test_time_functions_exist() {
        let content = fs::read_to_string("src/time.rs")
            .expect("Failed to read src/time.rs");
        let (functions, _, _) = extract_public_items(&content);

        assert!(functions.contains(&"now_ns".to_string()), "Missing now_ns function");
        assert!(functions.contains(&"now_us".to_string()), "Missing now_us function");
        assert!(functions.contains(&"now_ms".to_string()), "Missing now_ms function");
        assert!(functions.contains(&"now_secs_f64".to_string()), "Missing now_secs_f64 function");
    }
}

mod error_handling {
    use super::*;

    #[test]
    fn test_error_enum_exists() {
        let content = fs::read_to_string("src/syscall.rs")
            .expect("Failed to read src/syscall.rs");
        let (_, types, _) = extract_public_items(&content);

        assert!(types.contains(&"Error".to_string()), "Missing Error type");
    }

    #[test]
    fn test_error_variants_complete() {
        let content = fs::read_to_string("src/syscall.rs")
            .expect("Failed to read src/syscall.rs");

        // Check that all expected error variants are present
        let expected_variants = [
            "Success",
            "InvalidSyscall",
            "InvalidCapability",
            "PermissionDenied",
            "OutOfMemory",
            "InvalidArgument",
            "WouldBlock",
            "Timeout",
            "Interrupted",
            "NotFound",
            "InvalidFormat",
            "IoError",
            "TooManyProcesses",
            "NoChild",
            "BadAddress",
        ];

        for variant in &expected_variants {
            assert!(
                content.contains(variant),
                "Missing Error variant: {}",
                variant
            );
        }
    }
}

mod lib_exports {
    use super::*;

    #[test]
    fn test_lib_re_exports() {
        let content = fs::read_to_string("src/lib.rs")
            .expect("Failed to read src/lib.rs");

        // Check module declarations
        assert!(content.contains("pub mod cap"), "Missing cap module export");
        assert!(content.contains("pub mod ipc"), "Missing ipc module export");
        assert!(content.contains("pub mod memory"), "Missing memory module export");
        assert!(content.contains("pub mod process"), "Missing process module export");
        assert!(content.contains("pub mod syscall"), "Missing syscall module export");
        assert!(content.contains("pub mod tensor"), "Missing tensor module export");
        assert!(content.contains("pub mod thread"), "Missing thread module export");
        assert!(content.contains("pub mod time"), "Missing time module export");

        // Check prelude exists
        assert!(content.contains("pub mod prelude"), "Missing prelude module");
    }
}
