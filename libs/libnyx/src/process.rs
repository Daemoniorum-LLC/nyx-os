//! Process management
//!
//! Functions for spawning, managing, and waiting on processes.

use crate::syscall::{self, nr, Error};

/// Process ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ProcessId(pub u64);

impl ProcessId {
    /// Create from raw value
    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get raw value
    pub const fn as_raw(&self) -> u64 {
        self.0
    }
}

/// Get the current process ID
///
/// # Example
/// ```no_run
/// let pid = getpid()?;
/// println!("My PID: {}", pid.as_raw());
/// ```
pub fn getpid() -> Result<ProcessId, Error> {
    let result = unsafe { syscall::syscall0(nr::PROCESS_GETPID) };
    Error::from_raw(result).map(ProcessId)
}

/// Get the parent process ID
///
/// Returns 0 if there is no parent (init process).
pub fn getppid() -> Result<ProcessId, Error> {
    let result = unsafe { syscall::syscall0(nr::PROCESS_GETPPID) };
    Error::from_raw(result).map(ProcessId)
}

/// Spawn a new process
///
/// # Arguments
/// * `path` - Path to the executable
///
/// # Returns
/// The new process's PID
///
/// # Example
/// ```no_run
/// let child_pid = spawn("/bin/hello")?;
/// let (pid, exit_code) = wait(Some(child_pid))?;
/// ```
pub fn spawn(path: &str) -> Result<ProcessId, Error> {
    let result = unsafe {
        syscall::syscall5(
            nr::PROCESS_SPAWN,
            path.as_ptr() as u64,
            path.len() as u64,
            0, // args_ptr (not implemented)
            0, // args_len
            0, // flags
        )
    };

    Error::from_raw(result).map(ProcessId)
}

/// Exit the current process
///
/// This function does not return.
///
/// # Arguments
/// * `exit_code` - Exit code to return to parent
pub fn exit(exit_code: i32) -> ! {
    unsafe {
        syscall::syscall1(nr::PROCESS_EXIT, exit_code as u64);
    }
    // Should never reach here, but satisfy the return type
    loop {
        core::hint::spin_loop();
    }
}

/// Wait result containing child PID and exit code
#[derive(Clone, Copy, Debug)]
pub struct WaitResult {
    /// Child process ID
    pub pid: ProcessId,
    /// Exit code
    pub exit_code: i32,
}

/// Wait for a child process to exit
///
/// # Arguments
/// * `pid` - Specific PID to wait for, or None for any child
///
/// # Returns
/// The child's PID and exit code
///
/// # Example
/// ```no_run
/// // Wait for any child
/// let result = wait(None)?;
/// println!("Child {} exited with code {}", result.pid.as_raw(), result.exit_code);
///
/// // Wait for specific child
/// let result = wait(Some(child_pid))?;
/// ```
pub fn wait(pid: Option<ProcessId>) -> Result<WaitResult, Error> {
    let pid_arg = pid.map(|p| p.0).unwrap_or(0);
    let result = unsafe { syscall::syscall1(nr::PROCESS_WAIT, pid_arg) };

    Error::from_raw(result).map(|packed| {
        let exit_code = (packed >> 32) as i32;
        let child_pid = (packed & 0xFFFFFFFF) as u64;
        WaitResult {
            pid: ProcessId(child_pid),
            exit_code,
        }
    })
}
