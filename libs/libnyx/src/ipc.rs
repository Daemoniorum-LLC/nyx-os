//! IPC ring interface

use crate::syscall;

/// IPC Ring for async operations
pub struct IpcRing {
    fd: i32,
}

impl IpcRing {
    /// Create a new IPC ring
    pub fn new(sq_size: u32, cq_size: u32) -> Result<Self, i32> {
        let fd = unsafe {
            syscall::syscall2(
                syscall::nr::RING_SETUP,
                sq_size as u64,
                cq_size as u64,
            )
        };

        if fd < 0 {
            Err(fd as i32)
        } else {
            Ok(Self { fd: fd as i32 })
        }
    }

    /// Submit entries and wait for completions
    pub fn enter(&self, to_submit: u32, min_complete: u32) -> Result<u32, i32> {
        let ret = unsafe {
            syscall::syscall3(
                syscall::nr::RING_ENTER,
                self.fd as u64,
                to_submit as u64,
                min_complete as u64,
            )
        };

        if ret < 0 {
            Err(ret as i32)
        } else {
            Ok(ret as u32)
        }
    }
}

/// IPC Message
#[repr(C)]
pub struct Message {
    pub tag: u32,
    pub length: u32,
    pub data: [u8; 256],
}

impl Default for Message {
    fn default() -> Self {
        Self {
            tag: 0,
            length: 0,
            data: [0; 256],
        }
    }
}
