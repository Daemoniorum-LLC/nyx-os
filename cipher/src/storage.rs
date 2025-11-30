//! Secure storage utilities

use anyhow::{Result, anyhow};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Secure file operations
pub struct SecureFile;

impl SecureFile {
    /// Write data to file with restricted permissions
    pub fn write(path: &Path, data: &[u8]) -> Result<()> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create file with restricted permissions (0600)
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;

        file.write_all(data)?;
        file.sync_all()?;

        Ok(())
    }

    /// Read data from file
    pub fn read(path: &Path) -> Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        Ok(data)
    }

    /// Securely delete file (overwrite before removal)
    pub fn secure_delete(path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(path)?;
        let size = metadata.len() as usize;

        // Overwrite with zeros
        let mut file = OpenOptions::new()
            .write(true)
            .open(path)?;

        let zeros = vec![0u8; size];
        file.write_all(&zeros)?;
        file.sync_all()?;

        // Overwrite with ones
        let ones = vec![0xFFu8; size];
        file.write_all(&ones)?;
        file.sync_all()?;

        // Overwrite with random
        let mut random = vec![0u8; size];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut random);
        file.write_all(&random)?;
        file.sync_all()?;

        // Delete
        fs::remove_file(path)?;

        Ok(())
    }

    /// Set secure permissions on directory
    pub fn secure_dir(path: &Path) -> Result<()> {
        fs::create_dir_all(path)?;

        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)?;

        Ok(())
    }
}

use std::os::unix::fs::PermissionsExt;

/// Memory-locked allocation for sensitive data
#[cfg(target_os = "linux")]
pub mod locked {
    use std::alloc::{alloc, dealloc, Layout};

    /// Allocate memory and lock it (prevent swapping)
    pub fn alloc_locked(size: usize) -> Option<*mut u8> {
        let layout = Layout::from_size_align(size, 8).ok()?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return None;
            }

            // Lock memory
            if libc::mlock(ptr as *const libc::c_void, size) != 0 {
                dealloc(ptr, layout);
                return None;
            }

            Some(ptr)
        }
    }

    /// Unlock and deallocate memory
    pub fn free_locked(ptr: *mut u8, size: usize) {
        if ptr.is_null() {
            return;
        }

        let layout = match Layout::from_size_align(size, 8) {
            Ok(l) => l,
            Err(_) => return,
        };

        unsafe {
            // Zero memory
            std::ptr::write_bytes(ptr, 0, size);

            // Unlock
            libc::munlock(ptr as *const libc::c_void, size);

            // Deallocate
            dealloc(ptr, layout);
        }
    }
}
