//! Virtual Filesystem Layer
//!
//! Provides a unified interface for file operations across different
//! filesystem implementations (initrd, ext4, etc.)

mod initrd;

pub use initrd::{InitrdFs, InitrdError};

use crate::cap::{Capability, ObjectId, ObjectType, Rights};
use crate::mem::{PhysAddr, VirtAddr};
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::RwLock;

/// Global filesystem registry
static FILESYSTEMS: RwLock<BTreeMap<String, MountedFs>> = RwLock::new(BTreeMap::new());

/// Global initrd instance
static INITRD: RwLock<Option<InitrdFs>> = RwLock::new(None);

/// Mounted filesystem
struct MountedFs {
    /// Mount point
    mount_point: String,
    /// Filesystem type
    fs_type: FsType,
    /// Read-only flag
    read_only: bool,
}

/// Filesystem types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsType {
    /// Initial ramdisk (CPIO or TAR)
    Initrd,
    /// In-memory tmpfs
    Tmpfs,
    /// Device filesystem
    Devfs,
    /// Proc filesystem
    Procfs,
    /// Sysfs
    Sysfs,
}

/// File types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    Regular,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// FIFO (named pipe)
    Fifo,
    /// Socket
    Socket,
}

/// File metadata
#[derive(Clone, Debug)]
pub struct FileStat {
    /// File type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// Number of hard links
    pub nlink: u64,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Permission mode
    pub mode: u32,
    /// Last access time (Unix timestamp)
    pub atime: u64,
    /// Last modification time
    pub mtime: u64,
    /// Creation time
    pub ctime: u64,
    /// Device ID (for device files)
    pub dev: u64,
    /// Inode number
    pub ino: u64,
}

impl Default for FileStat {
    fn default() -> Self {
        Self {
            file_type: FileType::Regular,
            size: 0,
            nlink: 1,
            uid: 0,
            gid: 0,
            mode: 0o644,
            atime: 0,
            mtime: 0,
            ctime: 0,
            dev: 0,
            ino: 0,
        }
    }
}

/// Directory entry
#[derive(Clone, Debug)]
pub struct DirEntry {
    /// Entry name
    pub name: String,
    /// File type
    pub file_type: FileType,
    /// Inode number
    pub ino: u64,
}

/// Open file handle
pub struct FileHandle {
    /// Object ID
    pub object_id: ObjectId,
    /// Path
    pub path: String,
    /// Current position
    pub position: u64,
    /// Access flags
    pub flags: OpenFlags,
    /// File metadata
    pub stat: FileStat,
}

bitflags::bitflags! {
    /// File open flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct OpenFlags: u32 {
        /// Read access
        const READ = 1 << 0;
        /// Write access
        const WRITE = 1 << 1;
        /// Append mode
        const APPEND = 1 << 2;
        /// Create if not exists
        const CREATE = 1 << 3;
        /// Truncate existing file
        const TRUNCATE = 1 << 4;
        /// Fail if exists (with CREATE)
        const EXCL = 1 << 5;
        /// Non-blocking mode
        const NONBLOCK = 1 << 6;
        /// Directory
        const DIRECTORY = 1 << 7;
    }
}

/// Filesystem errors
#[derive(Debug, Clone)]
pub enum FsError {
    /// File not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Is a directory
    IsDirectory,
    /// Not a directory
    NotDirectory,
    /// File exists
    Exists,
    /// Read-only filesystem
    ReadOnly,
    /// No space left
    NoSpace,
    /// Invalid argument
    InvalidArgument,
    /// I/O error
    IoError,
    /// Not implemented
    NotImplemented,
    /// Filesystem not mounted
    NotMounted,
}

// ============================================================================
// Filesystem Operations
// ============================================================================

/// Initialize the filesystem subsystem
pub fn init() {
    log::debug!("Initializing filesystem subsystem");
    log::debug!("Filesystem subsystem initialized");
}

/// Initialize initrd from boot info
pub fn init_initrd(addr: PhysAddr, size: usize) {
    log::info!("Loading initrd at {:#x}, {} bytes", addr.as_u64(), size);

    // Map initrd into kernel address space
    let virt_addr = crate::arch::x86_64::paging::phys_to_virt(addr);

    // Create initrd filesystem
    let data = unsafe {
        core::slice::from_raw_parts(virt_addr.as_u64() as *const u8, size)
    };

    match InitrdFs::new(data) {
        Ok(fs) => {
            *INITRD.write() = Some(fs);

            // Mount at root
            FILESYSTEMS.write().insert(
                String::from("/"),
                MountedFs {
                    mount_point: String::from("/"),
                    fs_type: FsType::Initrd,
                    read_only: true,
                },
            );

            log::info!("Initrd mounted at /");
        }
        Err(e) => {
            log::error!("Failed to load initrd: {:?}", e);
        }
    }
}

/// Read a file completely
pub fn read_file(path: &str) -> Result<Vec<u8>, FsError> {
    // Normalize path
    let path = normalize_path(path);

    // Try initrd first
    if let Some(initrd) = INITRD.read().as_ref() {
        return initrd.read_file(&path);
    }

    Err(FsError::NotMounted)
}

/// Get file metadata
pub fn stat(path: &str) -> Result<FileStat, FsError> {
    let path = normalize_path(path);

    if let Some(initrd) = INITRD.read().as_ref() {
        return initrd.stat(&path);
    }

    Err(FsError::NotMounted)
}

/// List directory contents
pub fn readdir(path: &str) -> Result<Vec<DirEntry>, FsError> {
    let path = normalize_path(path);

    if let Some(initrd) = INITRD.read().as_ref() {
        return initrd.readdir(&path);
    }

    Err(FsError::NotMounted)
}

/// Check if a path exists
pub fn exists(path: &str) -> bool {
    stat(path).is_ok()
}

/// Check if a path is a directory
pub fn is_dir(path: &str) -> bool {
    stat(path).map(|s| s.file_type == FileType::Directory).unwrap_or(false)
}

/// Check if a path is a regular file
pub fn is_file(path: &str) -> bool {
    stat(path).map(|s| s.file_type == FileType::Regular).unwrap_or(false)
}

/// Open a file
pub fn open(path: &str, flags: OpenFlags) -> Result<FileHandle, FsError> {
    let path = normalize_path(path);
    let stat = stat(&path)?;

    // Check directory flag
    if flags.contains(OpenFlags::DIRECTORY) && stat.file_type != FileType::Directory {
        return Err(FsError::NotDirectory);
    }

    // Check if trying to write to read-only fs
    if flags.contains(OpenFlags::WRITE) {
        return Err(FsError::ReadOnly);
    }

    Ok(FileHandle {
        object_id: ObjectId::new(ObjectType::File),
        path,
        position: 0,
        flags,
        stat,
    })
}

/// Read from a file handle
pub fn read(handle: &mut FileHandle, buf: &mut [u8]) -> Result<usize, FsError> {
    let data = read_file(&handle.path)?;

    let start = handle.position as usize;
    if start >= data.len() {
        return Ok(0);
    }

    let available = data.len() - start;
    let to_read = core::cmp::min(buf.len(), available);

    buf[..to_read].copy_from_slice(&data[start..start + to_read]);
    handle.position += to_read as u64;

    Ok(to_read)
}

/// Seek in a file
pub fn seek(handle: &mut FileHandle, offset: i64, whence: SeekFrom) -> Result<u64, FsError> {
    let new_pos = match whence {
        SeekFrom::Start => offset as u64,
        SeekFrom::Current => {
            if offset >= 0 {
                handle.position.saturating_add(offset as u64)
            } else {
                handle.position.saturating_sub((-offset) as u64)
            }
        }
        SeekFrom::End => {
            if offset >= 0 {
                handle.stat.size.saturating_add(offset as u64)
            } else {
                handle.stat.size.saturating_sub((-offset) as u64)
            }
        }
    };

    handle.position = new_pos;
    Ok(new_pos)
}

/// Seek position reference
#[derive(Clone, Copy, Debug)]
pub enum SeekFrom {
    /// From beginning
    Start,
    /// From current position
    Current,
    /// From end
    End,
}

/// Normalize a path (remove .., ., double slashes)
fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                components.pop();
            }
            c => components.push(c),
        }
    }

    if components.is_empty() {
        String::from("/")
    } else {
        let mut result = String::new();
        for c in components {
            result.push('/');
            result.push_str(c);
        }
        result
    }
}
