//! Safe userspace memory access
//!
//! This module provides safe primitives for copying data between kernel and
//! userspace. All functions validate that pointers are within the userspace
//! address range and that the memory is actually mapped before access.
//!
//! ## Security Properties
//!
//! - **Range Validation**: All pointers are checked to be within userspace bounds
//! - **Mapping Validation**: Memory must be mapped before access (prevents kernel memory read)
//! - **Permission Checking**: Write operations verify write permission
//! - **Bounds Checking**: Length is validated to prevent overflows

use super::{VirtAddr, PAGE_SIZE};
use crate::process;
use alloc::string::String;
use alloc::vec::Vec;

/// Maximum address for userspace (canonical low half on x86_64)
const USER_SPACE_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Minimum address for userspace (avoid null pointer region)
const USER_SPACE_MIN: u64 = 0x1000;

/// Maximum allowed size for a single copy operation (16 MB)
const MAX_COPY_SIZE: usize = 16 * 1024 * 1024;

/// Errors that can occur during userspace memory access
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMemError {
    /// Pointer is null
    NullPointer,
    /// Pointer is outside userspace address range
    InvalidAddress,
    /// Memory region is not mapped
    NotMapped,
    /// Memory region lacks required permissions
    PermissionDenied,
    /// Requested size is too large
    SizeTooLarge,
    /// Address overflow during range calculation
    AddressOverflow,
}

/// Validate that an address range is within userspace bounds
///
/// Returns `Ok(())` if the entire range [ptr, ptr + len) is within valid
/// userspace address bounds, `Err` otherwise.
#[inline]
fn validate_user_range(ptr: u64, len: usize) -> Result<(), UserMemError> {
    // Reject null pointers
    if ptr == 0 {
        return Err(UserMemError::NullPointer);
    }

    // Reject excessive sizes
    if len > MAX_COPY_SIZE {
        return Err(UserMemError::SizeTooLarge);
    }

    // Zero-length is always valid (nothing to access)
    if len == 0 {
        return Ok(());
    }

    // Check start is in userspace
    if ptr < USER_SPACE_MIN || ptr > USER_SPACE_MAX {
        return Err(UserMemError::InvalidAddress);
    }

    // Check end doesn't overflow or exceed userspace
    let end = ptr
        .checked_add(len as u64)
        .ok_or(UserMemError::AddressOverflow)?;
    if end > USER_SPACE_MAX + 1 {
        return Err(UserMemError::InvalidAddress);
    }

    Ok(())
}

/// Check if a userspace address range is mapped with required permissions
///
/// This function walks the page tables of the current process to verify
/// that all pages in the range are mapped and have the required permissions.
fn verify_mapped(ptr: u64, len: usize, need_write: bool) -> Result<(), UserMemError> {
    if len == 0 {
        return Ok(());
    }

    // Get current process's address space
    let pid = process::current_process_id().ok_or(UserMemError::NotMapped)?;
    let proc = process::get_process(pid).ok_or(UserMemError::NotMapped)?;

    // Check each page in the range
    let start_page = ptr & !(PAGE_SIZE - 1);
    let end_page = (ptr + len as u64 - 1) & !(PAGE_SIZE - 1);
    let mut current = start_page;

    while current <= end_page {
        let virt = VirtAddr::new(current);

        // Try to translate - if it fails, the page isn't mapped
        if proc.address_space.translate(virt).is_none() {
            return Err(UserMemError::NotMapped);
        }

        // Check VMA permissions if we need write access
        if need_write {
            // Find VMA containing this address and check write permission
            let mut found_writable = false;
            for vma in proc.address_space.regions() {
                if vma.start.as_u64() <= current && current < vma.end.as_u64() {
                    if vma.protection.contains(super::virt::Protection::WRITE) {
                        found_writable = true;
                    }
                    break;
                }
            }
            if !found_writable {
                return Err(UserMemError::PermissionDenied);
            }
        }

        current = current.saturating_add(PAGE_SIZE);
        if current == 0 {
            break; // Overflow protection
        }
    }

    Ok(())
}

/// Safely copy data from userspace into a kernel-owned Vec
///
/// This function validates that the source pointer is within userspace bounds
/// and that the memory is mapped before performing the copy.
///
/// # Arguments
///
/// * `src` - Source pointer in userspace
/// * `len` - Number of bytes to copy
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - The copied data
/// * `Err(UserMemError)` - If validation fails
///
/// # Example
///
/// ```ignore
/// let data = copy_from_user(user_ptr as *const u8, user_len)?;
/// ```
pub fn copy_from_user(src: *const u8, len: usize) -> Result<Vec<u8>, UserMemError> {
    let src_addr = src as u64;

    // Validate the pointer range
    validate_user_range(src_addr, len)?;

    // Verify the memory is mapped
    verify_mapped(src_addr, len, false)?;

    // Now safe to copy
    let mut buffer = Vec::with_capacity(len);

    // SAFETY: We have verified that:
    // 1. src is within valid userspace bounds
    // 2. The memory is mapped
    // 3. len is within bounds
    unsafe {
        core::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), len);
        buffer.set_len(len);
    }

    Ok(buffer)
}

/// Safely copy a string from userspace
///
/// This is a convenience wrapper around `copy_from_user` that converts
/// the result to a String using lossy UTF-8 conversion.
///
/// # Arguments
///
/// * `src` - Source pointer in userspace
/// * `max_len` - Maximum number of bytes to copy
///
/// # Returns
///
/// * `Ok(String)` - The copied string
/// * `Err(UserMemError)` - If validation fails
pub fn copy_string_from_user(src: *const u8, max_len: usize) -> Result<String, UserMemError> {
    let data = copy_from_user(src, max_len)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

/// Safely copy data from kernel to userspace
///
/// This function validates that the destination pointer is within userspace
/// bounds and that the memory is mapped with write permissions.
///
/// # Arguments
///
/// * `dst` - Destination pointer in userspace
/// * `src` - Source data in kernel space
///
/// # Returns
///
/// * `Ok(())` - Copy succeeded
/// * `Err(UserMemError)` - If validation fails
pub fn copy_to_user(dst: *mut u8, src: &[u8]) -> Result<(), UserMemError> {
    let dst_addr = dst as u64;
    let len = src.len();

    // Validate the pointer range
    validate_user_range(dst_addr, len)?;

    // Verify the memory is mapped with write permission
    verify_mapped(dst_addr, len, true)?;

    // Now safe to copy
    // SAFETY: We have verified that:
    // 1. dst is within valid userspace bounds
    // 2. The memory is mapped with write permissions
    // 3. len is within bounds
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
    }

    Ok(())
}

/// Safely copy a single value from userspace
///
/// # Type Parameters
///
/// * `T` - The type to copy (must be Copy and sized)
///
/// # Arguments
///
/// * `src` - Source pointer in userspace
///
/// # Returns
///
/// * `Ok(T)` - The copied value
/// * `Err(UserMemError)` - If validation fails
pub fn copy_value_from_user<T: Copy>(src: *const T) -> Result<T, UserMemError> {
    let src_addr = src as u64;
    let len = core::mem::size_of::<T>();

    // Validate the pointer range
    validate_user_range(src_addr, len)?;

    // Verify the memory is mapped
    verify_mapped(src_addr, len, false)?;

    // SAFETY: We have verified the memory is valid and mapped
    unsafe { Ok(core::ptr::read(src)) }
}

/// Safely copy a single value to userspace
///
/// # Type Parameters
///
/// * `T` - The type to copy (must be Copy and sized)
///
/// # Arguments
///
/// * `dst` - Destination pointer in userspace
/// * `value` - The value to copy
///
/// # Returns
///
/// * `Ok(())` - Copy succeeded
/// * `Err(UserMemError)` - If validation fails
pub fn copy_value_to_user<T: Copy>(dst: *mut T, value: T) -> Result<(), UserMemError> {
    let dst_addr = dst as u64;
    let len = core::mem::size_of::<T>();

    // Validate the pointer range
    validate_user_range(dst_addr, len)?;

    // Verify the memory is mapped with write permission
    verify_mapped(dst_addr, len, true)?;

    // SAFETY: We have verified the memory is valid and mapped writable
    unsafe {
        core::ptr::write(dst, value);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_null_pointer() {
        assert_eq!(validate_user_range(0, 100), Err(UserMemError::NullPointer));
    }

    #[test]
    fn test_validate_kernel_address() {
        // Kernel addresses (high canonical half)
        assert_eq!(
            validate_user_range(0xFFFF_8000_0000_0000, 100),
            Err(UserMemError::InvalidAddress)
        );
    }

    #[test]
    fn test_validate_size_too_large() {
        assert_eq!(
            validate_user_range(0x1000, MAX_COPY_SIZE + 1),
            Err(UserMemError::SizeTooLarge)
        );
    }

    #[test]
    fn test_validate_overflow() {
        assert_eq!(
            validate_user_range(USER_SPACE_MAX, 100),
            Err(UserMemError::AddressOverflow)
        );
    }

    #[test]
    fn test_validate_valid_range() {
        assert!(validate_user_range(0x1000, 4096).is_ok());
        assert!(validate_user_range(0x0000_7000_0000_0000, 4096).is_ok());
    }

    #[test]
    fn test_validate_zero_length() {
        // Zero length should always succeed (nothing to access)
        assert!(validate_user_range(0x1000, 0).is_ok());
    }

    #[test]
    fn test_validate_range_end_at_boundary() {
        // Range that ends exactly at userspace boundary
        let end_addr = USER_SPACE_MAX - 99;
        assert!(validate_user_range(end_addr, 100).is_ok());
    }
}
